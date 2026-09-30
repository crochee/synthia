## 1. 移除 cache_breaker 字段与生成函数

- [x] 1.1 从 `SystemContext` 结构体中删除 `pub cache_breaker: String` 字段
- [x] 1.2 修改 `SystemContext::new()` 签名：从 `pub fn new(cache_breaker: String) -> Self` 改为 `pub fn new() -> Self`，移除函数体内的 `cache_breaker` 字段初始化
- [x] 1.3 删除私有函数 `generate_cache_breaker()`（含 `use rand::Rng` 和 `rand::thread_rng()` 调用）
- [x] 1.4 修改 `get_system_context()` 内的唯一调用点：将 `SystemContext::new(generate_cache_breaker())` 改为 `SystemContext::new()`

## 2. 修改测试适配新签名

- [x] 2.1 修改 `test_system_context_new`：改为 `SystemContext::new()` 无参数调用，删除 `assert_eq!(ctx.cache_breaker, "test_breaker")` 断言
- [x] 2.2 修改 `test_system_context_git_accessors`：将 `SystemContext::new("test".to_string())` 改为 `SystemContext::new()`
- [x] 2.3 删除 `test_cache_breaker_format` 测试函数
- [x] 2.4 保留 `test_clear_cache` 不变（与 cache_breaker 无关）

## 3. 验证与清理

- [x] 3.1 运行 `cargo check -p synthia-context` 确认编译通过
- [x] 3.2 检查 `synthia-context/Cargo.toml` 是否声明了 `rand` 依赖；若声明了且本 crate 不再使用，移除该依赖声明（当前未声明，预期无需改动）
- [x] 3.3 运行 `cargo test -p synthia-context` 确认全部测试通过
- [x] 3.4 运行 `cargo clippy -p synthia-context --all-targets --all-features --tests` 确认无警告
- [x] 3.5 运行 `cargo +nightly fmt --all` 格式化代码
- [x] 3.6 全仓库 grep `cache_breaker` 确认仅 openspec/changes/remove-cache-breaker/ 文档目录有匹配，源码无残留
