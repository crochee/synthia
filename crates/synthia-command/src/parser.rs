pub fn parse_command(input: &str) -> Option<(String, String)> {
    let input = input.trim();
    if !input.starts_with('/') {
        return None;
    }

    let rest = &input[1..];
    let parts: Vec<&str> = rest.splitn(2, ' ').collect();
    let name = parts[0].to_string();
    let args = parts.get(1).map(|s| s.to_string()).unwrap_or_default();

    Some((name, args))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_help_command() {
        let result = parse_command("/help");
        assert_eq!(result, Some(("help".to_string(), "".to_string())));
    }

    #[test]
    fn test_parse_help_with_args() {
        let result = parse_command("/help clear");
        assert_eq!(result, Some(("help".to_string(), "clear".to_string())));
    }

    #[test]
    fn test_parse_non_command() {
        let result = parse_command("hello world");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_model_command() {
        let result = parse_command("/model gpt-4");
        assert_eq!(result, Some(("model".to_string(), "gpt-4".to_string())));
    }

    #[test]
    fn test_parse_empty_slash() {
        let result = parse_command("/");
        assert_eq!(result, Some(("".to_string(), "".to_string())));
    }
}
