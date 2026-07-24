use serde::{Deserialize, Serialize};

/// 工具类别的超时/重试配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutorConfig {
    /// 默认超时时间（秒）
    pub default_timeout_secs: u64,
    /// 最大超时时间（秒）
    pub max_timeout_secs: u64,
    /// 重试次数
    pub max_retries: u32,
    /// 重试退避基数（秒）
    pub retry_base_secs: u64,
    /// 结果截断阈值（字节）
    pub truncate_threshold_bytes: usize,
    /// 截断时保留的头部字节数
    pub truncate_head_bytes: usize,
    /// 截断时保留的尾部字节数
    pub truncate_tail_bytes: usize,
}

impl Default for ToolExecutorConfig {
    fn default() -> Self {
        Self {
            default_timeout_secs: 60,
            max_timeout_secs: 600,
            max_retries: 2,
            retry_base_secs: 2,
            truncate_threshold_bytes: 16 * 1024, // 16KB
            truncate_head_bytes: 8 * 1024,       // 8KB head
            truncate_tail_bytes: 8 * 1024,       // 8KB tail
        }
    }
}

/// 按工具类别区分的超时配置
#[derive(Debug, Clone)]
pub struct ToolCategoryTimeout {
    /// 文件系统操作超时
    pub fs_timeout_secs: u64,
    /// Shell 命令超时
    pub shell_timeout_secs: u64,
    /// 网络请求超时
    pub web_timeout_secs: u64,
    /// 子 agent 超时
    pub subagent_timeout_secs: u64,
    /// MCP 工具超时
    pub mcp_timeout_secs: u64,
}

impl Default for ToolCategoryTimeout {
    fn default() -> Self {
        Self {
            fs_timeout_secs: 30,
            shell_timeout_secs: 60,
            web_timeout_secs: 45,
            subagent_timeout_secs: 300, // 5 minutes
            mcp_timeout_secs: 60,
        }
    }
}
