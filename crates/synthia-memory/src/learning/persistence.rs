//! The 4 persistence methods on
//! [`super::core::ExperienceLearner`]:
//!
//! - [`ExperienceLearner::serialize`] — JSON-encodes the
//!   `experiences` Vec (the `success_patterns` /
//!   `failure_patterns` are not persisted; they are
//!   derivable from the experiences).
//! - [`ExperienceLearner::deserialize`] — replaces the
//!   `experiences` Vec with the parsed JSON.
//! - [`ExperienceLearner::save_to_file`] — write the
//!   serialized JSON to a file path.
//! - [`ExperienceLearner::load_from_file`] — free
//!   function-style constructor that reads + parses +
//!   returns a fresh `ExperienceLearner`.

use std::{fs, path::Path};

use super::core::ExperienceLearner;

impl ExperienceLearner {
    pub fn serialize(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.experiences)
    }

    pub fn deserialize(&mut self, data: &str) -> Result<(), serde_json::Error> {
        self.experiences = serde_json::from_str(data)?;
        Ok(())
    }

    pub fn save_to_file<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<(), std::io::Error> {
        let data = self.serialize().map_err(std::io::Error::other)?;
        fs::write(path, data)?;
        Ok(())
    }

    pub fn load_from_file<P: AsRef<Path>>(
        path: P,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let data = fs::read_to_string(path)?;
        let mut learner = Self::new();
        learner.deserialize(&data)?;
        Ok(learner)
    }
}
