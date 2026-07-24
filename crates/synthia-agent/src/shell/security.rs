/// Shell 命令安全检测结果
#[derive(Debug, Clone)]
pub struct SecurityCheckResult {
    pub is_safe: bool,
    pub warnings: Vec<String>,
}

/// 检测危险命令模式
pub fn check_command_safety(command: &str) -> SecurityCheckResult {
    let mut warnings = Vec::new();
    let mut is_safe = true;

    // 检测解释器调用
    let interpreters =
        ["python", "python3", "perl", "node", "ruby", "bash", "sh"];
    for interp in &interpreters {
        if command.contains(interp) {
            warnings.push(format!("Contains {interp} interpreter call"));
        }
    }

    // 检测命令替换
    if command.contains("$(") || command.contains('`') {
        warnings.push("Contains command substitution".to_string());
    }

    // 检测 Base64 解码管道
    if command.contains("base64 -d") || command.contains("base64 --decode") {
        warnings.push("Contains base64 decode operation".to_string());
        is_safe = false;
    }

    // 检测 eval 执行
    if command.contains("eval ")
        || command.contains("| bash")
        || command.contains("| sh")
    {
        warnings.push("Contains eval or pipe to shell".to_string());
        is_safe = false;
    }

    SecurityCheckResult { is_safe, warnings }
}
