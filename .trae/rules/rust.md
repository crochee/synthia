---
alwaysApply: true
---
# Rust 编码规范

## 代码规范要求

新产生的 Rust 代码，如果没有使用的情况请删除，不能使用 `dead_code` 和`unused`标签忽略。

## 代码格式化

每次完成编写后需要执行以下命令进行代码格式化，确保代码风格一致：

```bash
cargo +nightly fmt --all
```

## 代码检查

格式化后执行以下命令检查代码是否符合 Rust 编码规范，并修复所有警告和错误：

```bash
cargo clippy --all-targets --all-features --tests --all
```

## 注意事项

- 修复所有 clippy 警告，确保代码通过检查
- 遵循 Rust 官方编码风格指南 (Rust Style Guide)
- 使用 `cargo fmt` 保持代码格式统一

# 运行规则
不要主动向我提问，自己探索最佳路径实施