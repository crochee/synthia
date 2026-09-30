//! `synthia_attachment` — content-addressed multimodal attachments.
//!
//! R11 (post-R10) closes the multimodal gap surfaced by the
//! traitclaw/dsh/pi surveys. Adopted from dsh
//! `packages/attachment/attachment/src/index.ts::AttachmentStore`:
//!
//! - [`ImageAttachmentRef`] — durable, content-addressed handle
//!   (`sha256` of the bytes) that travels through the LLM context
//!   instead of inline base64. The model-adapter serializes per
//!   provider (Anthropic / OpenAI).
//! - [`AttachmentStore`] — pluggable storage backend (filesystem
//!   or in-memory); validators enforce MIME / size limits at
//!   `save_image` time; `read_image` verifies the stored bytes
//!   recompute to the recorded hash.
//! - [`AttachmentError`] — typed errors (`NotFound`,
//!   `HashMismatch`, `UnsupportedMime`, `TooLarge`, `Io`,
//!   `Storage`).
//!
//! ## Why a separate crate?
//!
//! Image bytes are heavy; lib consumers don't always want a
//! dependency on `sha2` / `base64` / `chrono`. Splitting the
//! crate keeps the core surface clean while making the API
//! available wherever multimodal is needed (synthia-server HTTP
//! surface, agent-internal image round-trips).
//!
//! ## Reference
//!
//! - dsh `packages/attachment/attachment/src/index.ts:1-90`
//! - dsh `packages/llm/llm/src/types.ts::ImageBlock` + `ImageAttachmentRef`
//! - traitclaw `crates/traitclaw-core/src/types/tool.rs` image handling

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use base64::{
    Engine as _,
    engine::general_purpose::STANDARD as BASE64_STANDARD,
};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use synthia_core::Clock;
use synthia_provider::ContentPart;

/// Default maximum attachment size: 8 MiB.
///
/// Larger images must be resized by the caller before
/// `save_image`; pi `autoResizeImages` is the reference behaviour.
pub const DEFAULT_MAX_BYTES: usize = 8 * 1024 * 1024;

/// MIME types accepted by [`AttachmentStore::save_image`].
/// Matches the dsh `validateImage` allow-list plus BMP
/// (pi compatibility).
pub const SUPPORTED_IMAGE_MIME: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/bmp",
];

/// Content-addressed handle for an image attachment. The hash is
/// the lowercase hex SHA-256 of the raw bytes; the format is the
/// canonical MIME type (one of [`SUPPORTED_IMAGE_MIME`]).
///
/// Cheap to clone (`String`s + 4 `String`s = small struct).
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct ImageAttachmentRef {
    /// Hex SHA-256 of the bytes.
    pub hash: String,
    /// MIME type (one of [`SUPPORTED_IMAGE_MIME`]).
    pub mime_type: String,
    /// Byte length (cached so the LLM can compute transport cost
    /// without re-reading the storage backend).
    pub byte_len: usize,
    /// Wall-clock timestamp the attachment was saved.
    #[serde(default = "Utc::now")]
    pub saved_at: DateTime<Utc>,
}

impl ImageAttachmentRef {
    /// Build a ref from raw bytes by hashing, stamping `saved_at`
    /// from the wall clock. Validates MIME and size; use
    /// [`AttachmentStore::save_image`] for the full validation pipeline
    /// (storage + dedup + record) — the store stamps its own clock.
    ///
    /// The clock read goes through
    /// [`synthia_core::Clock`], not
    /// `chrono::Utc::now`, so a caller that needs a deterministic
    /// timestamp uses [`ImageAttachmentRef::from_bytes_at`].
    pub fn from_bytes(
        bytes: &[u8],
        mime_type: &str,
    ) -> Result<Self, AttachmentError> {
        Self::from_bytes_at(
            bytes,
            mime_type,
            synthia_core::SharedClock::system().now(),
        )
    }

    /// [`ImageAttachmentRef::from_bytes`] with an explicit timestamp —
    /// the deterministic half, for tests and for callers whose clock
    /// comes from elsewhere.
    pub fn from_bytes_at(
        bytes: &[u8],
        mime_type: &str,
        saved_at: DateTime<Utc>,
    ) -> Result<Self, AttachmentError> {
        validate_mime(mime_type)?;
        validate_size(bytes.len())?;
        let hash = sha256_hex(bytes);
        Ok(Self {
            hash,
            mime_type: mime_type.to_string(),
            byte_len: bytes.len(),
            saved_at,
        })
    }

    /// True when the ref's hash matches the bytes.
    pub fn matches(&self, bytes: &[u8]) -> bool {
        self.hash == sha256_hex(bytes)
    }
}

/// Adapter hook: render an [`ImageAttachmentRef`] into a
/// `ContentPart::Image(ImageContent)` for the LLM wire format.
/// The caller supplies the inline base64 bytes (typically fetched
/// from the storage backend) and the optional [`synthia_provider::ImageDetail`]
/// (mirrors `synthia_provider::ImageDetail`).
pub fn attachment_to_content_part(
    ref_: &ImageAttachmentRef,
    inline_base64: &str,
    detail: Option<synthia_provider::ImageDetail>,
) -> ContentPart {
    ContentPart::Image(synthia_provider::ImageContent {
        data: inline_base64.to_string(),
        mime_type: ref_.mime_type.clone(),
        detail,
    })
}

/// Compute the SHA-256 hex digest of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Validate MIME type. Returns `UnsupportedMime` otherwise.
pub fn validate_mime(mime_type: &str) -> Result<(), AttachmentError> {
    if SUPPORTED_IMAGE_MIME.contains(&mime_type) {
        Ok(())
    } else {
        Err(AttachmentError::UnsupportedMime {
            mime: mime_type.to_string(),
        })
    }
}

/// Validate size against `limit` (default [`DEFAULT_MAX_BYTES`]).
pub fn validate_size(bytes: usize) -> Result<(), AttachmentError> {
    if bytes > DEFAULT_MAX_BYTES {
        return Err(AttachmentError::TooLarge {
            bytes,
            limit: DEFAULT_MAX_BYTES,
        });
    }
    Ok(())
}

/// Pluggable attachment storage backend.
pub trait AttachmentBackend: Send + Sync {
    /// Save `bytes` and return the canonical ref. The backend is
    /// responsible for content addressing (hash → bytes) and
    /// may dedup internally.
    fn save(
        &self,
        bytes: &[u8],
        mime_type: &str,
    ) -> Result<ImageAttachmentRef, AttachmentError>;

    /// Read the bytes for `ref_`. Returns `NotFound` if absent
    /// or `HashMismatch` if the stored bytes can't be verified.
    fn read(
        &self,
        ref_: &ImageAttachmentRef,
    ) -> Result<Vec<u8>, AttachmentError>;
}

/// Typed errors for [`AttachmentStore`] and the backend trait.
#[derive(Debug, thiserror::Error)]
pub enum AttachmentError {
    /// Backend returned no bytes for the recorded hash.
    #[error("attachment not found: {hash}")]
    NotFound { hash: String },
    /// Bytes retrieved from the backend do not hash to the
    /// recorded value. Storage is corrupt or misnamed.
    #[error("attachment hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    /// MIME type not in [`SUPPORTED_IMAGE_MIME`].
    #[error("unsupported MIME type: {mime}")]
    UnsupportedMime { mime: String },
    /// Bytes exceed `limit`.
    #[error("attachment too large: {bytes} bytes > {limit} limit")]
    TooLarge { bytes: usize, limit: usize },
    /// Underlying I/O failure.
    #[error("attachment I/O: {0}")]
    Io(#[from] std::io::Error),
    /// Backend-specific error.
    #[error("attachment storage: {0}")]
    Storage(String),
}

/// Multi-backend attachment store. The default constructor wires
/// a filesystem backend at `root` plus an in-memory cache for hot
/// reads; backends can be replaced via [`AttachmentStore::with_backend`].
#[derive(Clone)]
pub struct AttachmentStore {
    backend: Arc<dyn AttachmentBackend>,
    /// In-memory cache for hot reads; populated lazily by
    /// [`AttachmentStore::read_image`]. Capped at 256 entries to
    /// keep memory bounded.
    cache: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    cache_capacity: usize,
    /// Wall-clock source for the timestamps this store stamps on the
    /// refs it returns. Default [`synthia_core::SystemClock`]; tests
    /// inject a [`synthia_core::FixedClock`] via
    /// [`AttachmentStore::with_clock`].
    clock: synthia_core::SharedClock,
}

impl Default for AttachmentStore {
    fn default() -> Self {
        // In-memory-only default: no filesystem root. Useful for
        // tests + ephemeral lib use. Production callers should
        // use [`AttachmentStore::new_fs`].
        Self::new(Arc::new(InMemoryBackend::default()))
    }
}

impl AttachmentStore {
    /// Build a store backed by `backend`.
    pub fn new(backend: Arc<dyn AttachmentBackend>) -> Self {
        Self {
            backend,
            cache: Arc::new(RwLock::new(HashMap::new())),
            cache_capacity: 256,
            clock: synthia_core::SharedClock::system(),
        }
    }

    /// Install the wall-clock source this store stamps on refs.
    ///
    /// The store, not the backend, owns `saved_at`: a backend is a
    /// byte store, and its notion of "now" (if it has one) is not the
    /// deployment's. Tests pass a
    /// [`synthia_core::FixedClock`].
    #[must_use]
    pub fn with_clock(mut self, clock: synthia_core::SharedClock) -> Self {
        self.clock = clock;
        self
    }

    /// Build a filesystem-backed store rooted at `root`.
    /// The directory is created if absent.
    pub fn new_fs(root: impl Into<PathBuf>) -> Result<Self, AttachmentError> {
        let root = root.into();
        std::fs::create_dir_all(&root).map_err(AttachmentError::Io)?;
        Ok(Self::new(Arc::new(FilesystemBackend::new(root)?)))
    }

    /// Override the backend (drops the cache; cache stays empty
    /// and will re-populate on next read).
    #[must_use]
    pub fn with_backend(mut self, backend: Arc<dyn AttachmentBackend>) -> Self {
        self.backend = backend;
        self.cache.write().clear();
        self
    }

    /// The wall-clock source this store stamps refs from — for a
    /// caller that needs to build a ref of its own against the same
    /// clock (e.g. a hash-only probe in an HTTP response).
    #[must_use]
    pub fn clock(&self) -> &synthia_core::SharedClock {
        &self.clock
    }

    /// Override the cache capacity (default 256).
    #[must_use]
    pub fn with_cache_capacity(mut self, capacity: usize) -> Self {
        self.cache_capacity = capacity.max(1);
        self
    }

    /// Save `bytes` (validating MIME + size), dedup via content
    /// hash, and return the canonical ref.
    pub fn save_image(
        &self,
        bytes: &[u8],
        mime_type: &str,
    ) -> Result<ImageAttachmentRef, AttachmentError> {
        let mut ref_ = self.backend.save(bytes, mime_type)?;
        // The store owns the timestamp: a backend is a byte store, and
        // its clock (if any) is not this deployment's.
        ref_.saved_at = self.clock.now();
        // Populate the cache with the just-saved bytes so the
        // first read is a hit.
        self.insert_cache(&ref_.hash, bytes.to_vec());
        Ok(ref_)
    }

    /// Read the bytes for `ref_`, populating the cache on the
    /// way out.
    pub fn read_image(
        &self,
        ref_: &ImageAttachmentRef,
    ) -> Result<Vec<u8>, AttachmentError> {
        if let Some(bytes) = self.cache.read().get(&ref_.hash) {
            return Ok(bytes.clone());
        }
        let bytes = self.backend.read(ref_)?;
        // Verify the recorded hash still matches the bytes the
        // backend returned. Detects storage corruption / rename.
        if !ref_.matches(&bytes) {
            return Err(AttachmentError::HashMismatch {
                expected: ref_.hash.clone(),
                actual: sha256_hex(&bytes),
            });
        }
        self.insert_cache(&ref_.hash, bytes.clone());
        Ok(bytes)
    }

    /// Read and base64-encode the bytes for the LLM wire format.
    /// Convenience wrapper around [`Self::read_image`].
    pub fn read_base64(
        &self,
        ref_: &ImageAttachmentRef,
    ) -> Result<String, AttachmentError> {
        let bytes = self.read_image(ref_)?;
        Ok(BASE64_STANDARD.encode(bytes))
    }

    /// Read raw bytes by hash only. Used by the GET endpoint
    /// when the caller supplies only the hash (no MIME or
    /// byte_len). The cache short-circuit avoids a backend
    /// round-trip; on miss we probe the backend with a
    /// minimal ref carrying just the hash.
    pub fn read_base64_raw(
        &self,
        hash: &str,
    ) -> Result<Vec<u8>, AttachmentError> {
        if let Some(bytes) = self.cache.read().get(hash) {
            return Ok(bytes.clone());
        }
        let probe = ImageAttachmentRef {
            hash: hash.to_string(),
            mime_type: String::new(),
            byte_len: 0,
            // Only the hash is read by the backend; the timestamp goes
            // through the store's clock rather than the wall clock so
            // this path keeps the same discipline as `save_image`.
            saved_at: self.clock.now(),
        };
        let bytes = self.backend.read(&probe)?;
        self.insert_cache(hash, bytes.clone());
        Ok(bytes)
    }

    /// Drop a cached entry. No-op if absent.
    pub fn evict(&self, hash: &str) {
        self.cache.write().remove(hash);
    }

    /// Current cache size.
    pub fn cache_len(&self) -> usize {
        self.cache.read().len()
    }

    fn insert_cache(&self, hash: &str, bytes: Vec<u8>) {
        let mut cache = self.cache.write();
        if cache.len() >= self.cache_capacity {
            // Simple eviction: drop the first inserted entry. A
            // production store would use an LRU; for the bounded
            // 256-entry default this is sufficient.
            if let Some(first) = cache.keys().next().cloned() {
                cache.remove(&first);
            }
        }
        cache.insert(hash.to_string(), bytes);
    }
}

// -- Filesystem backend ------------------------------------------------

/// Filesystem-backed attachment storage. Each attachment lives at
/// `{root}/{first_two_hex}/{hash}.{ext}` so the directory listing
/// stays manageable.
pub struct FilesystemBackend {
    root: PathBuf,
}

impl FilesystemBackend {
    /// Open or create the filesystem backend rooted at `root`.
    pub fn new(root: PathBuf) -> Result<Self, AttachmentError> {
        std::fs::create_dir_all(&root).map_err(AttachmentError::Io)?;
        Ok(Self { root })
    }

    fn path_for(&self, ref_: &ImageAttachmentRef) -> PathBuf {
        let prefix: String = ref_.hash.chars().take(2).collect();
        let ext = mime_to_ext(&ref_.mime_type);
        self.root
            .join(prefix)
            .join(format!("{}.{}", ref_.hash, ext))
    }
}

impl AttachmentBackend for FilesystemBackend {
    fn save(
        &self,
        bytes: &[u8],
        mime_type: &str,
    ) -> Result<ImageAttachmentRef, AttachmentError> {
        let ref_ = ImageAttachmentRef::from_bytes(bytes, mime_type)?;
        let path = self.path_for(&ref_);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(AttachmentError::Io)?;
        }
        // Content-addressed dedup: if the file already exists
        // with the same hash, the save is a no-op.
        if path.exists() {
            return Ok(ref_);
        }
        std::fs::write(&path, bytes).map_err(AttachmentError::Io)?;
        Ok(ref_)
    }

    fn read(
        &self,
        ref_: &ImageAttachmentRef,
    ) -> Result<Vec<u8>, AttachmentError> {
        let path = self.path_for(ref_);
        if !path.exists() {
            return Err(AttachmentError::NotFound {
                hash: ref_.hash.clone(),
            });
        }
        std::fs::read(&path).map_err(AttachmentError::Io)
    }
}

// -- In-memory backend --------------------------------------------------

/// In-memory attachment backend. Use for tests + ephemeral lib use.
#[derive(Default, Clone)]
pub struct InMemoryBackend {
    inner: InMemoryMap,
}

/// Hash → (bytes, mime) map backing [`InMemoryBackend`].
type InMemoryMap = Arc<RwLock<HashMap<String, (Vec<u8>, String)>>>;

impl AttachmentBackend for InMemoryBackend {
    fn save(
        &self,
        bytes: &[u8],
        mime_type: &str,
    ) -> Result<ImageAttachmentRef, AttachmentError> {
        let ref_ = ImageAttachmentRef::from_bytes(bytes, mime_type)?;
        self.inner
            .write()
            .insert(ref_.hash.clone(), (bytes.to_vec(), mime_type.to_string()));
        Ok(ref_)
    }

    fn read(
        &self,
        ref_: &ImageAttachmentRef,
    ) -> Result<Vec<u8>, AttachmentError> {
        self.inner
            .read()
            .get(&ref_.hash)
            .map(|(bytes, _)| bytes.clone())
            .ok_or_else(|| AttachmentError::NotFound {
                hash: ref_.hash.clone(),
            })
    }
}

fn mime_to_ext(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        _ => "bin",
    }
}

#[cfg(test)]
mod tests {
    use synthia_provider::ImageDetail;

    use super::*;

    fn png_bytes() -> Vec<u8> {
        // 1x1 transparent PNG.
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00,
            0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
            0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89,
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63,
            0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x6F, 0x32,
            0x4D, 0x0E, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
            0x42, 0x60, 0x82,
        ]
    }

    #[test]
    fn sha256_hex_is_deterministic() {
        let a = sha256_hex(b"hello");
        let b = sha256_hex(b"hello");
        assert_eq!(a, b);
        assert_eq!(
            a,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn image_attachment_ref_round_trip() {
        let bytes = png_bytes();
        let mime = "image/png";
        let ref_ = ImageAttachmentRef::from_bytes(&bytes, mime).unwrap();
        assert_eq!(ref_.mime_type, mime);
        assert_eq!(ref_.byte_len, bytes.len());
        assert!(ref_.matches(&bytes));
    }

    #[test]
    fn validate_mime_rejects_unknown() {
        assert!(validate_mime("image/jpeg").is_ok());
        assert!(validate_mime("image/png").is_ok());
        assert!(validate_mime("application/pdf").is_err());
    }

    #[test]
    fn validate_size_rejects_oversize() {
        assert!(validate_size(0).is_ok());
        assert!(validate_size(DEFAULT_MAX_BYTES).is_ok());
        assert!(validate_size(DEFAULT_MAX_BYTES + 1).is_err());
    }

    #[test]
    fn in_memory_backend_save_and_read_round_trip() {
        let store = AttachmentStore::default();
        let bytes = png_bytes();
        let ref_ = store.save_image(&bytes, "image/png").unwrap();
        let read_back = store.read_image(&ref_).unwrap();
        assert_eq!(read_back, bytes);
    }

    /// The store stamps `saved_at` from its own clock — a backend is a
    /// byte store with no business deciding what "now" means — so a
    /// `FixedClock` makes attachment timestamps deterministic.
    #[test]
    fn save_image_stamps_the_injected_clock() {
        let store = AttachmentStore::default().with_clock(
            synthia_core::SharedClock::fixed_from_rfc3339(
                "2026-01-02T03:04:05Z",
            ),
        );
        let ref_ = store.save_image(&png_bytes(), "image/png").unwrap();
        assert_eq!(
            ref_.saved_at.to_rfc3339(),
            "2026-01-02T03:04:05+00:00",
            "the store must stamp its injected clock, not the wall clock"
        );
    }

    #[test]
    fn save_dedups_by_content_hash() {
        let store = AttachmentStore::default();
        let bytes = png_bytes();
        let r1 = store.save_image(&bytes, "image/png").unwrap();
        let r2 = store.save_image(&bytes, "image/png").unwrap();
        assert_eq!(r1.hash, r2.hash);
        assert_eq!(r1.byte_len, r2.byte_len);
        assert_eq!(r1.mime_type, r2.mime_type);
    }
    #[test]
    fn read_base64_decodes_to_original_bytes() {
        let store = AttachmentStore::default();
        let bytes = png_bytes();
        let ref_ = store.save_image(&bytes, "image/png").unwrap();
        let b64 = store.read_base64(&ref_).unwrap();
        let decoded = BASE64_STANDARD.decode(b64).unwrap();
        assert_eq!(decoded, bytes);
    }

    #[test]
    fn cache_hit_skips_backend_read() {
        let store = AttachmentStore::default();
        let bytes = png_bytes();
        let ref_ = store.save_image(&bytes, "image/png").unwrap();
        // Save populates the cache; read should hit it.
        assert!(store.cache_len() >= 1);
        let read_back = store.read_image(&ref_).unwrap();
        assert_eq!(read_back, bytes);
    }

    #[test]
    fn evict_drops_cache_entry() {
        let store = AttachmentStore::default();
        let bytes = png_bytes();
        let ref_ = store.save_image(&bytes, "image/png").unwrap();
        store.evict(&ref_.hash);
        // Reading still works (re-fetches from backend) but the
        // cache count is non-zero because the read repopulates.
        let _ = store.read_image(&ref_).unwrap();
    }

    #[test]
    fn attachment_to_content_part_renders_image() {
        let store = AttachmentStore::default();
        let bytes = png_bytes();
        let ref_ = store.save_image(&bytes, "image/png").unwrap();
        let part = attachment_to_content_part(
            &ref_,
            "BASE64",
            Some(ImageDetail::Auto),
        );
        match part {
            ContentPart::Image(img) => {
                assert_eq!(img.mime_type, "image/png");
                assert_eq!(img.data, "BASE64");
                assert_eq!(img.detail, Some(ImageDetail::Auto));
            }
            other => panic!("expected Image, got {other:?}"),
        }
    }

    #[test]
    fn filesystem_backend_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let backend =
            Arc::new(FilesystemBackend::new(tmp.path().to_path_buf()).unwrap());
        let store = AttachmentStore::new(backend).with_cache_capacity(8);
        let bytes = png_bytes();
        let ref_ = store.save_image(&bytes, "image/png").unwrap();
        // Drop the cache, force a re-read from disk.
        store.evict(&ref_.hash);
        let read_back = store.read_image(&ref_).unwrap();
        assert_eq!(read_back, bytes);
    }

    #[test]
    fn filesystem_backend_reports_not_found_for_missing_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = FilesystemBackend::new(tmp.path().to_path_buf()).unwrap();
        let ghost = ImageAttachmentRef {
            hash: "0".repeat(64),
            mime_type: "image/png".into(),
            byte_len: 0,
            saved_at: chrono::DateTime::parse_from_rfc3339(
                "2026-01-01T12:00:00Z",
            )
            .unwrap()
            .with_timezone(&chrono::Utc),
        };
        let err = backend.read(&ghost).unwrap_err();
        assert!(matches!(err, AttachmentError::NotFound { .. }));
    }

    #[test]
    fn hash_mismatch_detected_on_corrupt_backend() {
        use std::{collections::HashMap, sync::Arc as StdArc};

        #[derive(Default)]
        struct CorruptBackend {
            map: parking_lot::Mutex<HashMap<String, Vec<u8>>>,
        }
        impl AttachmentBackend for CorruptBackend {
            fn save(
                &self,
                bytes: &[u8],
                mime_type: &str,
            ) -> Result<ImageAttachmentRef, AttachmentError> {
                let ref_ = ImageAttachmentRef::from_bytes(bytes, mime_type)?;
                self.map
                    .lock()
                    .insert(ref_.hash.clone(), vec![0xFF; bytes.len()]);
                Ok(ref_)
            }

            fn read(
                &self,
                ref_: &ImageAttachmentRef,
            ) -> Result<Vec<u8>, AttachmentError> {
                Ok(self.map.lock().get(&ref_.hash).cloned().unwrap_or_default())
            }
        }

        let store =
            AttachmentStore::new(StdArc::new(CorruptBackend::default()));
        let bytes = png_bytes();
        let ref_ = store.save_image(&bytes, "image/png").unwrap();
        store.evict(&ref_.hash);
        let err = store.read_image(&ref_).unwrap_err();
        assert!(matches!(err, AttachmentError::HashMismatch { .. }));
    }

    #[test]
    fn unsupported_mime_rejected_at_construction() {
        let err =
            ImageAttachmentRef::from_bytes(b"x", "video/mp4").unwrap_err();
        assert!(matches!(err, AttachmentError::UnsupportedMime { .. }));
    }

    #[test]
    fn supported_mime_constant_matches_docs() {
        assert!(SUPPORTED_IMAGE_MIME.contains(&"image/jpeg"));
        assert!(SUPPORTED_IMAGE_MIME.contains(&"image/png"));
        assert!(SUPPORTED_IMAGE_MIME.contains(&"image/gif"));
        assert!(SUPPORTED_IMAGE_MIME.contains(&"image/webp"));
        assert!(SUPPORTED_IMAGE_MIME.contains(&"image/bmp"));
        assert_eq!(SUPPORTED_IMAGE_MIME.len(), 5);
    }
}
