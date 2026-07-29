//! 一个基于 [sqlx](https://docs.rs/sqlx) 的**编译期 DAO 代码生成器**（面向 SQLite）。
//!
//! 在 `build.rs` 里连接一次数据库、调用一次 [`generate()`]，就能：
//!
//! - 根据表结构自动生成基础 CRUD（插入 / 查询 / 更新 / 删除），并按字段命名约定自动支持
//!   乐观锁（`version` 列）和逻辑删除（`deleted` / `deleted_at` / `deleted_by` 列）；
//! - 解析类 MyBatis 动态 SQL 的 `.sql` 文件（`@if` / `@each` / `@page` 等指令），生成对应的
//!   查询/更新函数；
//! - 生成结果统一格式化后写入 `OUT_DIR/dao.rs`，`include!` 进来即可直接调用，函数体用的是
//!   `sqlx::query!` 系列编译期检查宏，字段名写错、类型不对编译期就能发现。
//!
//! 生成的函数统一接受 `ctx: &mut sqlx_context::DbContext` 作为执行上下文，调用方需要额外依赖
//! [`sqlx-context`](https://crates.io/crates/sqlx-context) 这个crate 来提供 `DbContext` 类型。
//!
//! 完整的快速开始、DSL语法参考、字段类型映射规则见
//! [仓库 README](https://github.com/DAIRO-HY/sqlx-dsl-dao/blob/main/sqlx-dsl-dao/README.md)，
//! 一个可以直接 `cargo run` 的完整示例见仓库里的
//! [`sqlx-dsl-dao-sample`](https://github.com/DAIRO-HY/sqlx-dsl-dao/tree/main/sqlx-dsl-dao-sample)。

use crate::table_dao::table_util;
use sqlx::SqlitePool;
use std::collections::HashMap;

pub mod table_dao;
pub mod dsl_dao;
mod utils;
mod write_util;

/// 生成 DAO 相关代码：先根据表结构生成基础 CRUD，再根据 DSL 文件生成动态 SQL 函数，最后统一写入 OUT_DIR/dao.rs
///
/// `type_overrides`: 字段名 -> Rust类型 的覆盖规则,用于项目自定义的字段命名约定(如某些字段虽是DATETIME类型,但希望生成为String)
pub async fn generate(conn: SqlitePool, dsl_dir: &str, type_overrides: &HashMap<String, String>) {
    table_util::set_type_overrides(type_overrides);

    //得到某个数据库中所有表的Entity信息
    let table_entities = table_util::all_entities(&conn).await;

    for entity in table_entities {
        let mod_name = &entity.name;

        write_util::add_entity(mod_name, entity.clone());

        //生成插入函数的源代码
        write_util::add_method_source(mod_name, entity.make_insert_func());

        //生成查询函数的源代码
        write_util::add_method_source(mod_name, entity.make_select_one_func());

        //生成查询所有数据的函数的源代码
        write_util::add_method_source(mod_name, entity.make_select_all_func());

        //生成更新函数的源代码
        write_util::add_method_source(mod_name, entity.make_update_func());

        //生成删除函数的源代码
        write_util::add_method_source(mod_name, entity.make_set_delete_func());

        //生成删除忽略版本函数的源代码
        write_util::add_method_source(mod_name, entity.make_set_delete_ignore_version_func());

        //生成物理删除函数的源代码
        write_util::add_method_source(mod_name, entity.make_delete_func());
    }

    //根据DSL文件生成动态sql函数的源代码
    let dsl_list = dsl_dao::read_dsl::get_dsl_list(&conn, dsl_dir).await;
    for it in dsl_list {
        dsl_dao::dsl_code_util::make_source(&conn, &it).await;
    }

    write_util::write();
}
