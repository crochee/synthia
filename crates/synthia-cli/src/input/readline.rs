//! Simple readline wrapper - fall back to std::io for now
use std::io::{self, Write};

pub struct ReadlineHandler {
    history: Vec<String>,
}

impl ReadlineHandler {
    pub fn new(_history_path: std::path::PathBuf) -> io::Result<Self> {
        Ok(Self {
            history: Vec::new(),
        })
    }

    pub fn read_line(&mut self, prompt: &str) -> io::Result<String> {
        print!("{}", prompt);
        io::stdout().flush()?;

        let mut line = String::new();
        io::stdin().read_line(&mut line)?;

        let line = line.trim_end_matches(['\n', '\r']).to_string();

        if !line.is_empty() {
            self.history.push(line.clone());
        }

        Ok(line)
    }

    #[allow(dead_code)]
    pub fn save_history(
        &mut self,
        _path: &std::path::PathBuf,
    ) -> io::Result<()> {
        // History saving not implemented yet
        Ok(())
    }
}

impl Default for ReadlineHandler {
    fn default() -> Self {
        Self::new(std::path::PathBuf::new()).unwrap()
    }
}
