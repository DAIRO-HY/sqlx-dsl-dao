use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;

/// 和 DairoDFS 的 DBUpgrade.create() 保持一致的建表顺序
const SQL_FILES: [&str; 6] = [
    "dfs_file.sql",
    "storage_file.sql",
    "share.sql",
    "sql_log.sql",
    "user.sql",
    "user_token.sql",
];

#[async_std::main]
async fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();

    // 每次构建都重建数据库,保证表结构和 sql/create/ 下的DDL一致
    let db_path = Path::new(&out_dir).join("dfs.db");
    let _ = std::fs::remove_file(&db_path);
    let db_url = format!("sqlite://{}", db_path.display());
    let pool = SqlitePoolOptions::new()
        .connect_with(SqliteConnectOptions::from_str(&db_url).unwrap().create_if_missing(true))
        .await
        .unwrap();

    let create_dir = Path::new(&manifest_dir).join("sql/create");
    for file in SQL_FILES {
        let sql = std::fs::read_to_string(create_dir.join(file)).unwrap();
        sqlx::raw_sql(&sql).execute(&pool).await.unwrap();
    }

    // 和 DFS 一样:把 dfs_file.sql 里的表名替换成 dfs_file_delete 再执行一遍。
    // 文件里的唯一索引名 idx_name 会重名而失败,DFS 同样忽略了这个错误,只有表被建出来
    let sql = std::fs::read_to_string(create_dir.join("dfs_file.sql")).unwrap();
    let _ = sqlx::raw_sql(&sql.replace("dfs_file", "dfs_file_delete")).execute(&pool).await;

    let config = sqlx_dsl_dao::GenerateConfig::new()
        // user_token.version 是客户端版本号,不是乐观锁
        .disable_convention("user_token.version")
        // DFS 的旧节点按小写前缀 "insertintodfs_file(" 判断是否需要下载文件
        .lowercase_keywords(true);
    let dsl_dir = Path::new(&manifest_dir).join("dsl");
    sqlx_dsl_dao::generate_with_config(pool, dsl_dir.to_str().unwrap(), &config).await;

    println!("cargo:rustc-env=DATABASE_URL={}", db_url);
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=sql");
    println!("cargo:rerun-if-changed=dsl");
    println!("cargo:rerun-if-changed={}", db_path.display());
}
