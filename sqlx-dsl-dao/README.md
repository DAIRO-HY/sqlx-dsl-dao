# sqlx-dsl-dao

一个基于 [sqlx](https://github.com/launchbadge/sqlx) 的 **编译期 DAO 代码生成器**（面向 SQLite）。

在 `build.rs` 中调用一次 `generate()`，它会：

1. 连接你的 SQLite 数据库，读取表结构，自动生成基础 CRUD 函数（insert / select / update / delete，并按约定支持乐观锁和逻辑删除）；
2. 解析你写的类 MyBatis 动态 SQL `.sql` 文件（支持 `@if` / `@each` / `@page` 等指令），生成对应的查询/更新函数；
3. 把生成结果统一格式化（[prettyplease](https://crates.io/crates/prettyplease)）后写入 `OUT_DIR/dao.rs`，你在代码里用 `include!` 引入即可直接调用。

生成的函数体使用 `sqlx::query!` / `query_as!` / `query_scalar!` 等**编译期检查宏**，因此编译期就能发现字段名写错、类型不匹配等问题。

> 当前版本 `0.0.2`，只支持 **SQLite**。API 还在早期阶段，可能有breaking change。

## 目录

- [快速开始](#快速开始)
- [约定：DbContext](#约定dbcontext)
- [基础CRUD生成规则](#基础crud生成规则)
- [生成配置 GenerateConfig](#生成配置-generateconfig)
- [写SQL日志](#写sql日志)
- [DSL 语法](#dsl-语法)
- [字段类型映射](#字段类型映射)
- [注意事项](#注意事项)
- [完整示例](#完整示例)

## 快速开始

### 1. 添加依赖

```toml
[dependencies]
sqlx-context = "0.0.2"
sqlx = { version = "0.8", features = ["runtime-async-std", "sqlite", "macros"] }

[build-dependencies]
sqlx-dsl-dao = "0.0.2"
sqlx = { version = "0.8", features = ["runtime-async-std", "sqlite", "macros"] }
async-std = { version = "1", features = ["attributes"] }
```

> `sqlx-dsl-dao 0.0.2` 生成的代码需要 `sqlx-context 0.0.2`（写SQL日志相关的API）。
>
> `sqlx` 的 `runtime-async-std` / `runtime-tokio` 只能二选一，且要和你项目其它地方用到的 sqlx 保持一致（Cargo 会统一整个依赖图的 feature）。

### 2. 写 `build.rs`

```rust
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::HashMap;
use std::str::FromStr;

#[async_std::main]
async fn main() {
    let db_url = "sqlite://./my_app.db";

    let opts = SqliteConnectOptions::from_str(db_url)
        .unwrap()
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new().connect_with(opts).await.unwrap();

    // 字段名 -> Rust类型 的覆盖规则；用于项目里一些特殊的字段命名约定
    let type_overrides: HashMap<String, String> = HashMap::new();

    // dsl_dir 目录下的所有 .sql 文件都会被解析
    sqlx_dsl_dao::generate(pool, "dsl", &type_overrides).await;

    // 编译期的 sqlx::query! 宏 和 运行期代码 需要用同一个 DATABASE_URL 才能对上号
    println!("cargo:rustc-env=DATABASE_URL={}", db_url);

    // 数据库/DSL目录发生变化，或数据库文件被删除时都要重新生成
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=my_app.db");
    println!("cargo:rerun-if-changed=dsl");
}
```

**注意**：`generate()` 是"读取现成的表结构"，它不会帮你建表——数据库和表必须在调用它之前就已经存在（上面例子里表结构假设已经通过别的方式建好；实际项目通常会在 `generate()` 之前先执行一遍建表 SQL）。

### 3. 写 DSL 文件（可选）

`dsl/user.sql`：

```sql
-- @name find_user_by_condition
-- @param name:String? 姓名（模糊匹配）
-- @param min_age:i64? 最小年龄
-- @return List<User>
SELECT * FROM user WHERE deleted = 0
-- @if name != null
AND name LIKE '%' || :name || '%'
-- @end
-- @if min_age != null
AND age >= :min_age
-- @end
ORDER BY id DESC
```

### 4. 在代码里引入生成结果

```rust
// crate 根（lib.rs / main.rs），不能嵌套在其它 mod 里
use sqlx_context::DbContext;

include!(concat!(env!("OUT_DIR"), "/dao.rs"));

async fn demo(ctx: &mut DbContext) -> Result<(), sqlx::Error> {
    let id = user_dao::insert(ctx, &user_dao::User {
        name: "Alice".into(),
        email: "alice@example.com".into(),
        ..Default::default()
    }).await?;

    let user = user_dao::select_one(ctx, id).await?;
    let list = user_dao::find_user_by_condition(ctx, Some("a".into()), Some(18)).await?;

    Ok(())
}
```

## 约定：DbContext

生成的每个函数第一个参数固定是 `ctx: &mut sqlx_context::DbContext`。这是一个裸路径（不是 `crate::sqlx_context`），所以**必须**依赖 [`sqlx-context`](https://crates.io/crates/sqlx-context) 这个crate——它把 `SqlitePool` 和事务统一封装成一个类型，并让 `&mut DbContext` 直接实现 `sqlx::Executor`，Rust 的 extern prelude 会让 `sqlx_context::DbContext` 这个路径在任意嵌套模块里都能直接解析到，不需要额外的 `use`。

```rust
use sqlx_context::DbContext;

let pool = /* SqlitePool */;
let mut ctx = DbContext::new(pool);
// ctx.begin() / ctx.commit() / ctx.rollback() 支持事务
```

## 基础CRUD生成规则

对数据库里的每一张表，都会生成一个 `pub mod {表名}_dao { ... }`，以及对应的实体结构体（表名转帕斯卡命名，如 `user` -> `User`）。

始终生成：

| 函数 | 说明 |
|---|---|
| `insert(ctx, entity)` | 插入。若有自增主键会用 `RETURNING` 返回主键值 |
| `select_one(ctx, 主键...)` | 按主键查询一条 |
| `select_all(ctx)` | 查询全部 |
| `update(ctx, entity)` | 按主键更新（若有 `version` 列，自动加上乐观锁条件） |
| `delete(ctx, 主键...)` | 物理删除 |

根据列名自动识别以下约定，触发额外行为：

| 列名 | 类型建议 | 行为 |
|---|---|---|
| `created_at` | `INTEGER`（毫秒时间戳） | `insert()` 自动写入当前时间，不需要调用方传值 |
| `updated_at` | `INTEGER`（毫秒时间戳） | `update()` 自动写入当前时间 |
| `version` | `INTEGER` | `update()` 自动 `version = version + 1` 并在 WHERE 中要求 `version = ?`（乐观锁） |
| `deleted` | `BOOLEAN` | 出现该列时，`select_all()` 只查未删除数据，并额外生成 `select_all_include_deleted()`；同时生成两个逻辑删除函数： `set_delete(ctx, 主键..., [version], [deleted_by])`（校验乐观锁）和 `set_delete_ignone_version(ctx, 主键..., [deleted_by])`（忽略乐观锁，*注意：当前版本函数名里 "ignore" 拼成了 "ignone"，是已知typo*） |
| `deleted_at` | `INTEGER`（毫秒时间戳） | 逻辑删除时自动写入删除时间 |
| `deleted_by` | `VARCHAR`/`TEXT` | 逻辑删除函数会多一个 `deleted_by: String` 参数 |

自增主键列（`INTEGER PRIMARY KEY AUTOINCREMENT`）不会出现在 `insert()` 的参数里；非自增主键（如 `INT8 PRIMARY KEY NOT NULL`，由业务生成）会和其它列一起插入。`insert()` / `update()` 的列顺序和建表语句里的列顺序一致。

以上约定可以通过 [`GenerateConfig::disable_convention`](#生成配置-generateconfig) 按表、按列关闭，关闭后该列就当作普通列处理。

## 生成配置 GenerateConfig

需要更多控制时用 `generate_with_config()` 代替 `generate()`：

```rust
let config = sqlx_dsl_dao::GenerateConfig::new()
    // 字段名 -> Rust类型 的覆盖规则，等同于 generate() 的第三个参数
    .type_override("property", "String")
    // user_token 表的 version 列是客户端版本号，不是乐观锁
    .disable_convention("user_token.version")
    // 表DAO生成的SQL关键字使用小写
    .lowercase_keywords(true);
sqlx_dsl_dao::generate_with_config(pool, "dsl", &config).await;
```

| 方法 | 说明 |
|---|---|
| `type_override(列名, Rust类型)` / `type_overrides(&HashMap)` | 按列名强制指定 Rust 类型 |
| `disable_convention(规则)` | 关闭约定字段的特殊处理，规则可以是 `"version"`（所有表的该列）、`"user_token.version"`（某张表的某列）、`"user_token.*"`（某张表的全部约定）、`"*"`（全部关闭）。列名只能是 `version` / `created_at` / `updated_at` / `deleted` / `deleted_at` / `deleted_by`，写错会在 build.rs 里 panic |
| `lowercase_keywords(true)` | 表DAO生成的SQL关键字使用小写（`insert into t (...) values (...)`、`update t set ... where ...`、`delete from`、`select`），默认大写。DSL 文件里手写的SQL不受影响，始终原样保留 |

关闭约定后的行为（以 `version` 为例）：`insert()` 会把它作为普通列插入，`update()` 会把它作为普通列更新，不再自动 `+1`，也不加乐观锁条件。

## 写SQL日志

用于把每一条成功执行的写SQL及其参数记录下来（比如存到一张日志表里，供其它节点重放）。

```rust
ctx.enable_write_log();
ctx.begin().await?;
dfs_file_dao::insert(&mut ctx, &file).await?;
dfs_file_dao::update_state(&mut ctx, id, 1, None).await?;

// commit 之前取出
for log in ctx.take_write_log() {
    // log.sql:    实际执行的SQL，占位符为 ?
    // log.params: Vec<serde_json::Value>，按绑定顺序排列
    println!("{} {}", log.sql, log.params_json()); // insert into dfs_file (...) values (?, ...) [1,"a.mp4",null,...]
}
ctx.commit().await?;
```

- 记录范围：表DAO的 `insert` / `update` / `delete` / `set_delete` / `set_delete_ignone_version`，以及 DSL 里所有非 `SELECT` 的函数。查询不记录。
- 只在执行成功后记录（`update` 影响 0 行也算执行成功，同样会记录）；执行失败不记录。
- 事务 `rollback()` 时，会丢弃本次事务期间记录的日志。
- 记录的是**真正执行的那一句SQL**：DSL 里 `@if` / `@each` 展开之后是什么就记什么；DSL 里写的SQL不改大小写、不重新格式化（只会把多行拼成一行）；表DAO的SQL受 `lowercase_keywords` 控制。
- 参数通过 `sqlx_context::log_value()` 转换，复用 sqlx 自己的编码结果：整数/浮点数 -> number，字符串 -> string，`None` -> `null`，`bool` -> `true`/`false`，`Vec<u8>` -> 标准 base64 字符串。
- 没有开启时，每个写函数只多一次 `is_write_log_enabled()` 判断，不会构建参数。
- 手写的SQL也可以调用 `ctx.push_write_log(sql, params)` 加入日志。
- 把取出的日志再用生成的DAO写入日志表时，这些写入本身也会被记录；可以在写日志表前 `ctx.disable_write_log()`，或者再 `take_write_log()` 一次丢弃。

## DSL 语法

在 `.sql` 文件里，用 `-- ------` （以 `-- ` 开头、后面跟若干个 `-`）分隔多个函数定义。每个函数用注释指令描述元信息，其余非注释行就是 SQL 本身。

| 指令 | 说明 |
|---|---|
| `-- @name 函数名` | 生成的函数名 |
| `-- @param 参数名:类型` | 声明一个参数；类型后加 `?` 表示 `Option`，支持 `List<T>` 映射为 `Vec<T>`。省略时会尝试从SQL里用到的 `:xxx` 占位符自动补全，类型依次按：查询的返回列、数据库里同名的表字段（各表中类型一致时）、参数名启发式规则推断。建议写SQL的参数都显式声明类型 |
| `-- @param_type 类型名` | 把所有参数打包进一个结构体（超过4个参数时会自动生成一个） |
| `-- @return 类型` | 返回值类型；`List<T>` 表示返回 `Vec<T>`，`T?` 表示返回 `Option<T>`；省略时会按查询列自动推断（多列会生成一个 `xxxTemp` 结构体） |
| `-- @page` | 分页查询，函数会多一个 `page_in: PageIn` 参数，返回 `(总数i64, Vec<T>)` |
| `-- @if 条件 && 条件 ... ` … `-- @end` | 按条件动态拼接一段SQL；条件写法是 `字段 运算符 值`（如 `name != null`、`age > 0`），可用 `&&`/`\|\|` 连接多个 |
| `-- @each 集合参数 seq="," open="(" close=")" item="it"` … `-- @end` | 遍历集合参数拼接SQL（比如 `IN (...)`）；`item` 是循环内使用的变量名 |

SQL 里用 `:参数名` 引用参数（会被转换成 `?` 绑定参数，SQL文本本身对SQLite来说也是合法的命名参数写法）。

一个使用 `@each` 生成 `IN` 查询的例子：

```sql
-- @name find_user_by_ids
-- @param ids:List<i64>
-- @return List<User>
SELECT * FROM user WHERE deleted = 0
-- @each ids seq="," open="AND id IN (" close=")" item="id"
:id
-- @end
```

> `@each`/`@if` 包裹的部分会被排除在"结果列自动推断"所用的SQL之外，所以 `open` 里的关键字（如上面的 `AND id IN (`）要写在 `@each` 属性里，而不要写在外层普通SQL文本里，否则外层SQL会因为片段不完整而无法通过 `describe()` 校验。

## 字段类型映射

类型名不区分大小写：

| 数据库类型 | Rust类型 |
|---|---|
| `INTEGER` / `INT` / `BIGINT` / `INT8` / `INT16` / `INT32` / `INT64` | `i64` |
| `VARCHAR` / `VARCHAR(n)` / `CHAR(n)` / `TEXT` | `String` |
| `BOOLEAN` | `bool` |
| `FLOAT` | `f32` |
| `DOUBLE` | `f64` |
| `DATETIME` | `chrono::NaiveDateTime` |
| 以上都不匹配，且列名以 `date` 结尾 | `chrono::NaiveDateTime` |
| 其它 | 原样输出类型名（需要用 `type_override` 指定） |

可为空的列（未声明 `NOT NULL`）会自动包一层 `Option<T>`。

如果内置规则不满足需求，`generate()` 的第三个参数 `type_overrides: &HashMap<String, String>` 可以按**列名**强制指定 Rust 类型。

## 注意事项

- 建表语句的解析兼容：`COLLATE NOCASE`、行尾注释（包括逗号和 `--` 紧贴的写法）、显式 `NULL`、小写的 `text` / `default` / `unique`、表级 `UNIQUE` / `PRIMARY KEY` / `FOREIGN KEY` / `CONSTRAINT` / `CHECK` 约束行。索引（包括带 `WHERE` 的部分唯一索引）不影响生成。
- DSL 动态SQL函数内部使用的局部变量都以 `__` 开头（`__sql`、`__args` 等），DSL 参数名不要以 `__` 开头。

- `include!(concat!(env!("OUT_DIR"), "/dao.rs"))` **必须放在 crate 根**：分页函数里用 `super::PageIn` 引用公共结构体，嵌套在其它 `mod` 里会导致 `super` 指向错误的模块。
- 生成的代码里大量使用 `sqlx::query!` 等编译期检查宏，这要求编译当前crate时 `DATABASE_URL` 环境变量指向一个**已经建好表结构**的真实数据库。推荐在 `build.rs` 里用 `println!("cargo:rustc-env=DATABASE_URL=...")` 注入，而不是依赖外部 `.env`。
- `cargo:rerun-if-changed=...` 一旦显式声明，Cargo 就只认清单里列出的路径，不再用"包内任意文件变化都重跑"的默认策略。如果你的数据库文件是 `build.rs` 生成的临时文件，**记得把数据库文件本身也加进监听列表**，否则它被误删后 Cargo 会因为"监听路径都没变"而跳过重新生成，导致运行期报 `unable to open database file`。
- `@if` / `@each` 包裹的SQL片段不参与"结果列类型"的自动推断（这部分推断只用剩余的静态SQL去做 `describe()`），所以要保证去掉这些动态片段之后剩下的SQL仍然是完整、合法的。

## 完整示例

仓库里的 [`sqlx-dsl-dao-sample`](../sqlx-dsl-dao-sample) 是一个可以直接 `cargo run` 的完整示例，覆盖了基础CRUD、乐观锁、逻辑删除、`@if`/`@each`/`@page`、关闭约定字段、写SQL日志等全部特性，`cargo test -p sqlx-dsl-dao-sample` 可以跑对应的测试。

[`sqlx-dsl-dao-dfs-compat`](../sqlx-dsl-dao-dfs-compat) 用一组线上实际使用的建表语句验证兼容性（小写关键字 + 写SQL日志）。

## License

<!-- TODO: 发布前需要在 Cargo.toml 里补上 license / description / repository 等 crates.io 必填的元数据，并在仓库根目录添加对应的 LICENSE 文件。 -->
