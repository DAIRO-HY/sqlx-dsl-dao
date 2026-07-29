use crate::table_dao::model::{CrudType, table};
use sqlx::{Column, Executor, Row, SqliteConnection, SqlitePool, Statement, TypeInfo};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// 字段名 -> Rust类型 的覆盖规则,由使用方通过 `generate()` 传入(项目自定义的字段命名约定无法内置在库里)
static TYPE_OVERRIDES: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 设置字段名 -> Rust类型 的覆盖规则,供 `generate()` 在生成前调用
pub(crate) fn set_type_overrides(overrides: &HashMap<String, String>) {
    *TYPE_OVERRIDES.lock().unwrap() = overrides.clone();
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
    if sql.starts_with("IF NOT EXISTS") {
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
    if column_name.to_uppercase() == "UNIQUE"
        || column_name.to_uppercase() == "PRIMARY"
        || column_name.to_uppercase() == "FOREIGN"
    {
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
    //优先使用调用方通过 generate() 传入的字段名覆盖规则
    if let Some(rust_type) = TYPE_OVERRIDES.lock().unwrap().get(name) {
        return rust_type.clone();
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
