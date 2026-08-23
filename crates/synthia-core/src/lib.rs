pub mod error;
pub mod registry;
pub mod sensitive;
pub mod text;
pub mod token;

/// Crate-wide [`Result`] alias defaulting to the single error
/// type [`Error`]. Callers returning a different error type can
/// still override the parameter (`synthia_core::Result<T, E>`),
/// mirroring the `std::io::Result` convention.
pub type Result<T, E = Error> = core::result::Result<T, E>;
pub use registry::{
    Registry,
    RegistryItem,
    RegistryList,
    paginate_registry_list,
};
pub use sensitive::{Sensitive, SensitiveData};
pub use text::cap_to_char_boundary;

pub use crate::error::Error;
