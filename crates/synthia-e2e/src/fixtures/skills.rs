#[derive(Debug, Clone)]
pub struct TestSkill {
    pub name: String,
    pub description: String,
    pub level: u8,
    pub keywords: Vec<String>,
    pub content: String,
}

impl TestSkill {
    pub fn rust_developer() -> Self {
        Self {
            name: "rust-developer".to_string(),
            description: "Skill for Rust programming language development"
                .to_string(),
            level: 2,
            keywords: vec![
                "rust".to_string(),
                "cargo".to_string(),
                "rustc".to_string(),
                " lifetimes".to_string(),
                "traits".to_string(),
                "ownership".to_string(),
            ],
            content: r#"# Rust Developer Skill

## Overview
This skill provides guidance for Rust programming including best practices,
common patterns, and idiomatic Rust code.

## Capabilities
- Writing idiomatic Rust code
- Understanding ownership and borrowing
- Working with traits and generics
- Error handling with Result and Option
- Async programming with Tokio

## Common Patterns

### Error Handling
```rust
fn read_file(path: &str) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}
```

### Async/Await
```rust
async fn fetch_data(url: &str) -> Result<String, reqwest::Error> {
    reqwest::get(url).await?.text().await
}
```
"#
            .to_string(),
        }
    }

    pub fn python_expert() -> Self {
        Self {
            name: "python-expert".to_string(),
            description: "Skill for Python programming language development"
                .to_string(),
            level: 2,
            keywords: vec![
                "python".to_string(),
                "pip".to_string(),
                "venv".to_string(),
                "decorators".to_string(),
                "async".to_string(),
            ],
            content: r#"# Python Expert Skill

## Overview
This skill provides guidance for Python programming including best practices,
common patterns, and Pythonic code.

## Capabilities
- Writing Pythonic code
- Type hints and dataclasses
- Async/await patterns
- Testing with pytest
- Package management

## Common Patterns

### Dataclass
```python
from dataclasses import dataclass

@dataclass
class User:
    name: str
    email: str
    active: bool = True
```

### Async Context Manager
```python
import asynccontextmanager

@asynccontextmanager
async def get_connection():
    conn = await create_connection()
    try:
        yield conn
    finally:
        await conn.close()
```
"#
            .to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_developer_skill() {
        let skill = TestSkill::rust_developer();
        assert_eq!(skill.name, "rust-developer");
        assert!(skill.keywords.contains(&"ownership".to_string()));
        assert!(skill.content.contains("ownership"));
    }

    #[test]
    fn test_python_expert_skill() {
        let skill = TestSkill::python_expert();
        assert_eq!(skill.name, "python-expert");
        assert!(skill.keywords.contains(&"async".to_string()));
    }
}
