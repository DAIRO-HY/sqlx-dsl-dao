use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

#[async_std::main]
async fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();

    // 每次构建都重新生成一个干净的示例数据库，避免生成的DAO代码与库结构不一致
    let db_path = Path::new(&manifest_dir).join("sample.db");
    let _ = std::fs::remove_file(&db_path);

    let db_url = format!("sqlite://{}", db_path.display());

    let connect_options = SqliteConnectOptions::from_str(&db_url)
        .unwrap()
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .connect_with(connect_options)
        .await
        .unwrap();

    // 按 ; 分割建表语句并逐条执行
    let schema_path = Path::new(&manifest_dir).join("schema.sql");
    let schema_sql = std::fs::read_to_string(&schema_path).unwrap();
    for stmt in schema_sql.split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        sqlx::query(stmt).execute(&pool).await.unwrap();
    }

    // 字段名 -> Rust类型 的覆盖规则，示例项目中数据库字段类型都能被默认规则正确推断，故留空
    let type_overrides: HashMap<String, String> = HashMap::new();

    let dsl_dir = Path::new(&manifest_dir).join("dsl");
    sqlx_dsl_dao::generate(pool, dsl_dir.to_str().unwrap(), &type_overrides).await;

    // 让编译期的 sqlx::query!/query_as!/query_scalar! 宏，以及运行期代码都能找到同一个示例数据库
    println!("cargo:rustc-env=DATABASE_URL={}", db_url);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=schema.sql");
    // sample.db 是build.rs的生成物而非源码，Cargo默认不会追踪它；
    // 显式监听它是为了当它被手动删除（不存在）时，Cargo也会判定为“变化”从而重新执行build.rs
    println!("cargo:rerun-if-changed=sample.db");
    println!("cargo:rerun-if-changed=dsl");
    for entry in std::fs::read_dir(Path::new(&manifest_dir).join("dsl")).unwrap() {
        let entry = entry.unwrap();
        println!("cargo:rerun-if-changed={}", entry.path().display());
    }
}
