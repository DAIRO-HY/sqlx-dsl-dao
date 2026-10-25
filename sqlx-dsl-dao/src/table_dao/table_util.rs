use crate::table_dao::model::{CrudType, table};
use sqlx::{Column, Executor, Row, SqliteConnection, SqlitePool, Statement, TypeInfo};
use crate::config;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// 所有表的 列名 -> Rust类型(同名列在不同表里类型不一致时为 None)
static SCHEMA_COLUMNS: LazyLock<Mutex<HashMap<String, Option<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 记录所有表的列类型,供DSL推断未声明类型的参数
pub(crate) fn set_schema_columns(entities: &[table::Entity]) {
    let mut map: HashMap<String, Option<String>> = HashMap::new();
    for column in entities.iter().flat_map(|it| it.columns.iter()) {
        let ty = db_type_to_rust(&column.data_type, &column.name);
        map.entry(column.name.clone())
            .and_modify(|it| {
                if it.as_ref() != Some(&ty) {
                    *it = None;
                }
            })
            .or_insert(Some(ty));
    }
    *SCHEMA_COLUMNS.lock().unwrap() = map;
}

/// 按列名从表结构中查找Rust类型
pub(crate) fn schema_column_type(name: &str) -> Option<String> {
    SCHEMA_COLUMNS.lock().unwrap().get(name).cloned().flatten()
}

/// 读取某个数据库中所有表Entity
pub async fn all_entities(conn: &SqlitePool) -> Vec<table::Entity> {
    sqlx::query(
        "SELECT name,sql FROM sqlite_master WHERE type = 'table' and name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(conn)
    .await
    .unwrap()
    .iter()
    .map(|it| {
        let sql: String = it.get("sql");
        let table_info = read_table_info_from_sql(&sql);
        table_info
    })
    .collect()
}

/// 通过sql预编译获取列信息
/// 适用于复杂SQL文中提取列信息
pub async fn get_column_from_sql(conn: &SqlitePool, sql: &str) -> Vec<table::Column> {
    let Ok(describe) = conn.describe(sql).await else {
        panic!("获取列数据失败:{}", sql);
    };
    describe
        .columns()
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let is_nullable = describe.nullable(i).unwrap_or(true);
            let name = it.name();
            if name.contains("!:") {
                //兼容sqlx的特殊标记，如exists!: bool
                let name_arr: Vec<&str> = name.split("!:").collect();
                table::Column {
                    name: name_arr[0].to_string(),
                    data_type: name_arr[1].to_string(),
                    is_nullable: false,
                    ..Default::default()
                }
            } else if name.contains(":") {
                //兼容sqlx的特殊标记，如exists: bool
                let name_arr: Vec<&str> = name.split(":").collect();
                table::Column {
                    name: name_arr[0].to_string(),
                    data_type: name_arr[1].to_string(),
                    is_nullable: true,
                    ..Default::default()
                }
            } else {
                table::Column {
                    name: name.to_string(),
                    data_type: it.type_info().name().to_string(),
                    is_nullable,
                    ..Default::default()
                }
            }
        })
        .collect()
}

/// 从建表语句的SQL中提取表信息
/// 适用于sqlite数据库
pub fn read_table_info_from_sql(sql: &str) -> table::Entity {
    let mut sql = sql.replace("\r\n", "\n");
    sql = sql.replace("\r", "\n");
    let (name, comment) = find_table_name(&sql);

    let open_kh_index = sql.find('(').unwrap(); //寻找第一个括号索引
    let close_kh_index = sql.rfind(')').unwrap(); //寻找最后一个括号索引
    let columns_sql = sql[open_kh_index + 1..close_kh_index].to_string();
    let columns = columns_sql
        .split('\n')
        .filter_map(|line| read_column_info_from_sql_line(line))
        .collect::<Vec<_>>();
    table::Entity {
        name,
        columns,
        comment,
    }
}

/// 从SQL语句中提取表名和表注释
/// 适用于sqlite
fn find_table_name(sql: &str) -> (String, String) {
    let mut sql = sql.to_string();

    // 将多个空格替换为一个空格
    while sql.contains("  ") {
        sql = sql.replace("  ", " ");
    }
    sql = sql.replace("\r\n", "\n");
    sql = sql.replace("\r", "\n");
    let create_table_index = sql.to_uppercase().find("CREATE TABLE").unwrap();
    sql = sql[create_table_index + 12..].trim().to_string();
    if sql.to_uppercase().starts_with("IF NOT EXISTS") {
        sql = sql[13..].trim().to_string();
    }

    let open_kh_index = sql.find('(').unwrap(); //寻找第一个括号索引
    let block_sql = sql[..open_kh_index].to_string();
    let comment_index = block_sql.find("--"); //寻找注释标记

    let mut comment = String::new();
    let name = if let Some(comment_index) = comment_index {
        comment = block_sql[comment_index..].trim().to_string();
        block_sql[..comment_index].trim().to_string()
    } else {
        block_sql.trim().to_string()
    };
    (name, comment)
}

/// 从SQL语句的列定义行中提取列信息
fn read_column_info_from_sql_line(line: &str) -> Option<table::Column> {
    let line = line.to_string();

    // 提取注释
    let comment_index = line.find("--");
    let (line, comment) = if let Some(index) = comment_index {
        (
            line[..index].trim().to_string(),
            line[index + 2..].trim().to_string(),
        )
    } else {
        (line.trim().to_string(), "".to_string())
    };
    let mut line = line;
    while line.contains("  ") {
        line = line.replace("  ", " ");
    }

    // 提取列名
    let next_space_index = line.find(' ').unwrap_or(line.len());
    let column_name = &line[..next_space_index];
    if column_name.is_empty() {
        return None;
    }

    // 列名只能包含字母、数字和下划线
    if !column_name
        .replace("_", "")
        .chars()
        .all(|c| c.is_ascii_alphanumeric())
    {
        return None;
    }
    if matches!(
        column_name.to_uppercase().as_str(),
        "UNIQUE" | "PRIMARY" | "FOREIGN" | "CONSTRAINT" | "CHECK"
    ) {
        return None;
    }

    // 提取数据类型
    let line = line[next_space_index..].trim().to_string();
    let next_space_index = line
        .find(' ')
        .unwrap_or(line.len())
        .min(line.find(',').unwrap_or(line.len()));
    let data_type = &line[..next_space_index];

    let line = line[next_space_index..].trim().to_string().to_uppercase();
    let is_primary_key = line.contains("PRIMARY KEY"); // 判断是否包含 PRIMARY KEY
    // 注：这里刻意不对主键做隐式 NOT NULL 特殊处理——sqlx 对 SQLite 的可空性推断在
    // 不同查询形态下并不完全一致(部分能证明 NOT NULL，部分不能)，如果让实体结构体
    // 强制声明为非 Option，会在推断不出 NOT NULL 的查询里产生类型不匹配；保持和
    // sqlx 实际推断结果一致(可空)更稳妥，业务代码里访问 id 字段时按需 unwrap。
    let is_nullable = !line.contains("NOT NULL"); // 判断是否包含 NOT NULL
    let is_auto_increment = line.contains("AUTOINCREMENT") || line.contains("AUTO_INCREMENT"); // 判断是否包含 AUTOINCREMENT 或 AUTO_INCREMENT

    Some(table::Column {
        name: column_name.to_string(),
        nick: "".to_string(),
        data_type: data_type.to_string(),
        is_primary_key,
        default_value: None,
        is_nullable,
        is_auto_increment,
        comment,
    })
}

// /// 通过sql预编译获取列信息
// pub async fn get_column_from_sql(conn: &SqlitePool, sql: &str) -> Vec<table::Column> {
//
//
//     let describe = conn.describe(sql)
//         .await.unwrap();
//
//     for (i,column) in describe.columns().iter().enumerate() {
//         println!("name = {},type = {:?},nullable={}", column.name(),column.type_info(), describe.nullable(i).unwrap());
//     }
//
//     conn.prepare(sql)
//         .await
//         .unwrap()
//         .columns()
//         .iter()
//         .map(|it| {
//             let is_nullable = match it.type_info().name() {
//                 "INTEGER" => false, //数字类型强制不允许为NULL
//                 _ => true,
//             };
//             let name = it.name().to_string();
//             if name.contains("!:") {
//                 //兼容sqlx的特殊标记，如exists!: bool
//                 let name_arr: Vec<&str> = name.split("!:").collect();
//                 table::Column {
//                     name: name_arr[0].to_string(),
//                     data_type: name_arr[1].to_string(),
//                     is_nullable: false,
//                     ..Default::default()
//                 }
//             } else if name.contains(":") {
//                 //兼容sqlx的特殊标记，如exists: bool
//                 let name_arr: Vec<&str> = name.split(":").collect();
//                 table::Column {
//                     name: name_arr[0].to_string(),
//                     data_type: name_arr[1].to_string(),
//                     is_nullable: true,
//                     ..Default::default()
//                 }
//             } else {
//                 table::Column {
//                     name,
//                     data_type: it.type_info().name().to_string(),
//                     is_nullable,
//                     ..Default::default()
//                 }
//             }
//         })
//         .collect::<Vec<_>>()
// }

#[derive(Debug, Clone, Copy)]
enum ParseSqlArgsState {
    Normal,
    SingleQuote,
    DoubleQuote,
}

/// 解析sql文中的参数
pub fn parse_sql_args(sql: &str) -> (String, Vec<String>) {
    let chars: Vec<char> = sql.chars().collect();

    let mut state = ParseSqlArgsState::Normal;

    let mut args = Vec::new();
    let mut output = String::new();

    let mut i = 0;

    while i < chars.len() {
        match state {
            ParseSqlArgsState::Normal => match chars[i] {
                '\'' => {
                    output.push(chars[i]);
                    state = ParseSqlArgsState::SingleQuote;
                }
                '"' => {
                    output.push(chars[i]);
                    state = ParseSqlArgsState::DoubleQuote;
                }
                ':' => {
                    // PostgreSQL ::text
                    if (i > 0 && chars[i - 1] == ':')
                        || (i + 1 < chars.len() && chars[i + 1] == ':')
                    {
                        output.push(chars[i]);
                    } else {
                        let start = i + 1;

                        if start < chars.len()
                            && (chars[start].is_ascii_alphabetic() || chars[start] == '_')
                        {
                            let mut end = start + 1;

                            while end < chars.len()
                                && (chars[end].is_ascii_alphanumeric() || chars[end] == '_')
                            {
                                end += 1;
                            }

                            // 保存参数名
                            args.push(chars[start..end].iter().collect());

                            // 替换成 ?
                            output.push('?');

                            // 跳过参数名
                            i = end - 1;
                        } else {
                            output.push(chars[i]);
                        }
                    }
                }
                _ => {
                    output.push(chars[i]);
                }
            },

            ParseSqlArgsState::SingleQuote => {
                output.push(chars[i]);

                if chars[i] == '\'' {
                    // SQL中''表示转义
                    if i + 1 < chars.len() && chars[i + 1] == '\'' {
                        i += 1;
                        output.push(chars[i]);
                    } else {
                        state = ParseSqlArgsState::Normal;
                    }
                }
            }

            ParseSqlArgsState::DoubleQuote => {
                output.push(chars[i]);

                if chars[i] == '"' {
                    // SQL中""表示转义
                    if i + 1 < chars.len() && chars[i + 1] == '"' {
                        i += 1;
                        output.push(chars[i]);
                    } else {
                        state = ParseSqlArgsState::Normal;
                    }
                }
            }
        }

        i += 1;
    }
    (output, args)
}

/// 将数据库数据类型映射为Rust类型
pub fn db_type_to_rust(db_type: &str, name: &str) -> String {
    //优先使用调用方通过 GenerateConfig 配置的字段名覆盖规则
    if let Some(rust_type) = config::type_override(name) {
        return rust_type;
    }
    match db_type.to_uppercase().as_str() {
        "INTEGER" | "INT" => "i64".to_string(),
        "BIGINT" => "i64".to_string(),
        "INT8" => "i64".to_string(),
        "INT16" => "i64".to_string(),
        "INT32" => "i64".to_string(),
        "INT64" => "i64".to_string(),
        "VARCHAR" | "TEXT" => "String".to_string(),
        "BOOLEAN" => "bool".to_string(),
        "FLOAT" => "f32".to_string(),
        "DOUBLE" => "f64".to_string(),
        "DATETIME" => "chrono::NaiveDateTime".to_string(),
        _ => {
            if db_type.to_uppercase().starts_with("VARCHAR")
                || db_type.to_uppercase().starts_with("CHAR")
            {
                "String".to_string()
            } else if name.to_lowercase().ends_with("date") {
                "chrono::NaiveDateTime".to_string()
            } else {
                db_type.to_string()
            }
        } // 默认使用String类型
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_dfs_file_ddl() {
        let entity = read_table_info_from_sql(
            "CREATE TABLE dfs_file
(
    id          INT8 PRIMARY KEY            NOT NULL,           -- 主键
    userId      INT8                        NOT NULL,-- 所属用户ID
    parentId    INT8                        NOT NULL DEFAULT 0, -- 父目录ID,当isExtra=1时
    name        VARCHAR(256) COLLATE NOCASE NOT NULL,           -- 名称(比较时忽略大小写)
    contentType VARCHAR(32)                 NULL,               -- 文件类型(文件专用)
    property    text                        NULL,
    thumb       INT8,-- 缩略图
    state       INTEGER          NOT NULL default 1,
    apiToken    VARCHAR(20) unique, -- API操作TOKEN
    stateMsg    TEXT                        NULL                -- 文件处理出错信息
)",
        );
        assert_eq!(entity.name, "dfs_file");
        let columns: Vec<(&str, &str, bool, bool, bool)> = entity
            .columns
            .iter()
            .map(|it| {
                (
                    it.name.as_str(),
                    it.data_type.as_str(),
                    it.is_primary_key,
                    it.is_auto_increment,
                    it.is_nullable,
                )
            })
            .collect();
        assert_eq!(
            columns,
            [
                ("id", "INT8", true, false, false),
                ("userId", "INT8", false, false, false),
                ("parentId", "INT8", false, false, false),
                ("name", "VARCHAR(256)", false, false, false),
                ("contentType", "VARCHAR(32)", false, false, true),
                ("property", "text", false, false, true),
                ("thumb", "INT8", false, false, true),
                ("state", "INTEGER", false, false, false),
                ("apiToken", "VARCHAR(20)", false, false, true),
                ("stateMsg", "TEXT", false, false, true),
            ]
        );
        assert_eq!(entity.columns[1].comment, "所属用户ID");
    }
}
