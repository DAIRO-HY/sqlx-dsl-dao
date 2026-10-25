# sqlx-dsl-dao

[![Crates.io](https://img.shields.io/crates/v/sqlx-dsl-dao.svg)](https://crates.io/crates/sqlx-dsl-dao)
[![Docs.rs](https://docs.rs/sqlx-dsl-dao/badge.svg)](https://docs.rs/sqlx-dsl-dao)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

一个基于 [sqlx](https://github.com/launchbadge/sqlx) 的 **编译期 DAO 代码生成器**（面向 SQLite）。

在 `build.rs` 里连接一次数据库、调用一次 `sqlx_dsl_dao::generate()`，就能：

- 根据表结构自动生成基础 CRUD（插入 / 查询 / 更新 / 删除），并按字段命名约定自动支持乐观锁（`version`）和逻辑删除（`deleted` / `deleted_at` / `deleted_by`）；
- 解析你写的类 MyBatis 动态 SQL `.sql` 文件（`@if` / `@each` / `@page` 等指令），生成对应的查询/更新函数；
- 生成结果统一格式化后写入 `OUT_DIR/dao.rs`，`include!` 进来即可直接调用，函数体用的是 `sqlx::query!` 系列编译期检查宏，字段名写错、类型不对编译期就能发现。

## 项目结构

本仓库是一个 Cargo workspace，包含两个crate：

| crate | 说明 |
|---|---|
| [`sqlx-dsl-dao`](./sqlx-dsl-dao) | 核心库，发布到 [crates.io](https://crates.io/crates/sqlx-dsl-dao)。详细用法、DSL语法、字段类型映射规则见 [sqlx-dsl-dao/README.md](./sqlx-dsl-dao/README.md) |
| [`sqlx-dsl-dao-sample`](./sqlx-dsl-dao-sample) | 可以直接 `cargo run` 的完整示例，覆盖基础CRUD、乐观锁、逻辑删除、`@if`/`@each`/`@page` 等全部特性 |
| [`sqlx-dsl-dao-dfs-compat`](./sqlx-dsl-dao-dfs-compat) | 兼容性测试（不发布）：用 DairoDFS 线上使用中的建表语句原样生成代码，验证类型映射、建表语句解析、关闭约定字段、小写关键字和写SQL日志。`cargo test -p sqlx-dsl-dao-dfs-compat` |

配套依赖 [`sqlx-context`](https://crates.io/crates/sqlx-context) 提供生成代码统一使用的 `DbContext` 执行上下文（封装了 `SqlitePool` 和事务）。

## 快速开始

```bash
git clone https://github.com/DAIRO-HY/sqlx-dsl-dao.git
cd sqlx-dsl-dao
cargo run -p sqlx-dsl-dao-sample
```

`sqlx-dsl-dao-sample` 的 `build.rs` 会自动创建示例数据库、建表、生成DAO代码，`main.rs` 会依次跑一遍插入、查询、动态条件查询、`IN`查询、分页查询、乐观锁更新、逻辑/物理删除、写SQL日志等全部功能，终端直接能看到每一步的输出。

在自己的项目里使用这个库，请看 [sqlx-dsl-dao/README.md](./sqlx-dsl-dao/README.md) 的完整文档（依赖配置、`build.rs` 写法、DSL语法参考、字段命名约定等）。

## License

本项目采用 [MIT](./LICENSE-MIT) 或 [Apache-2.0](./LICENSE-APACHE) 双许可，你可以任选其一使用。
