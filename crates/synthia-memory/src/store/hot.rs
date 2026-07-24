//! Hot-memory layer methods on [`super::builder::MemoryStoreImpl`].

impl super::builder::MemoryStoreImpl {
    pub async fn write_hot(
        &self,
        key: &str,
        content: &str,
    ) -> Result<(), synthia_core::Error> {
        self.hot.write(key, content).await
    }

    pub async fn read_hot(
        &self,
        key: &str,
    ) -> Result<Option<String>, synthia_core::Error> {
        self.hot.read(key).await
    }
}
