use crate::dsl_dao::model::dsl_function::DslFunction;
use crate::dsl_dao::model::dsl_block::{DslBlock, DslController};
use crate::dsl_dao::model::dsl_each::DslEach;
use crate::dsl_dao::model::dsl_if_condition::DslIfCondition;
use crate::dsl_dao::model::dsl_param::DslParam;
use sqlx::{SqliteConnection, SqlitePool};
use std::fs;
use std::path::PathBuf;

pub async fn get_dsl_list(conn: &SqlitePool, dsl_dir: &str) -> Vec<DslFunction> {
    let dsl_list = find_sql_paths(dsl_dir)
        .iter()
        .fold(Vec::new(), |mut b, sql_path| {
            // 文件名
            let file_name = sql_path.file_name().unwrap().to_string_lossy().to_string();

            //从文件中获取DSL函数列表
            let mut list = parse_dsl_block(sql_path)
                .iter()
                .map(|it| parse_dsl(file_name.clone(), it))
                .collect::<Vec<_>>();
            b.append(&mut list);
            b
        });
    let mut fix_dsl_list = Vec::new();
    for it in dsl_list {
        // 修复一些默认配置,比如参数补足
        fix_dsl_list.push(it.fix(conn).await);
    }
    fix_dsl_list
}

/// 获取所有sql文件的path
fn find_sql_paths(dsl_dir: &str) -> Vec<PathBuf> {
    let Ok(read_dir) = fs::read_dir(dsl_dir) else {
        return Default::default();
    };

    //遍历目录下的所有sql文件
    read_dir
        .filter_map(Result::ok)
        .filter_map(|it| {
            let Ok(file_type) = it.file_type() else {
                //可能文件被删除或者没有访问权限
                return None;
            };
            if file_type.is_dir() {
                return None;
            }
            if !it
                .file_name()
                .to_string_lossy()
                .to_lowercase()
                .ends_with(".sql")
            {
                return None;
            }
            Some(it.path())
        })
        .collect()
}

/// 从sql路径中解析dsl代码块
fn parse_dsl_block(sql_path: &PathBuf) -> Vec<String> {
    //保存当前文件的代码块列表
    let mut block_code_list: Vec<String> = Vec::new();
    let mut temp_clock = String::new();
    fs::read_to_string(sql_path)
        .unwrap()
        .lines()
        .for_each(|line| {
            if line.trim().is_empty(){
                return;
            }
            if line.starts_with("-- ------") && !temp_clock.is_empty() {
                block_code_list.push(std::mem::take(&mut temp_clock));
            } else {
                temp_clock.push_str(line);
                temp_clock.push('\n');
            }
        });
    if !temp_clock.is_empty() {
        block_code_list.push(temp_clock);
    }
    block_code_list
}

/// 解析代码块中解析sql操作函数
fn parse_dsl(file_name: String, block_dsl: &str) -> DslFunction {
    let mut name = "".to_string(); //函数名
    let mut is_page = false; //是否是分页查询
    let mut return_type = "".to_string(); //返回值类型
    let mut return_list = false; //返回值是否是一个列表
    let mut return_option = false; //返回值是否可选
    let mut params = Vec::new(); //参数列表
    let mut param_type = None; //函数参数类型名称(结构体名,类名)
    let mut comment = Vec::new(); //注释信息

    // 条件渲染部分的sql
    let mut block_list = Vec::new();

    // 条件渲染部分的sql
    // let mut each_block_list = Vec::new();

    let lines = block_dsl.lines().collect::<Vec<&str>>();

    //条件渲染的子sql
    let mut temp_sql = String::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("-- @param ") {
            if let Some(param) = parse_dsl_param_by_line(line) {
                params.push(param);
            }
        } else if line.starts_with("-- @name ") {
            name = line[9..].trim().to_string();
            name = name.split_whitespace().next().unwrap().to_string();
        } else if line.starts_with("-- @param_type ") {
            param_type = Some(line[15..].trim().to_string());
        } else if line.starts_with("-- @return ") {
            //解析函数返回值
            return_type = line[11..].trim().to_string();
            return_type = return_type.split_whitespace().next().unwrap().to_string();

            if return_type.to_lowercase().starts_with("list<") {
                //如果返回值是一个列表
                return_list = true;
                return_type = return_type[5..return_type.len() - 1].trim().to_string();
            } else if return_type.to_lowercase() == "list" {
                //如果返回值是一个列表
                return_list = true;
                return_type = String::new();
            }
            if return_type.ends_with("?"){
                return_option = true;
                return_type.truncate(return_type.len() - 1)
            }
            return_type = return_type.trim().to_string();
        } else if line.starts_with("-- @page") {
            is_page = true;
        } else if line.starts_with("-- @if") {
            //条件渲染
            if !temp_sql.is_empty() {
                //添加一个无条件渲染的子sql
                block_list.push(DslBlock {
                    controller: None,
                    sql: std::mem::take(&mut temp_sql),
                });
            }

            //去解析条件
            let block = parse_if_condition(&lines, &mut i);
            block_list.push(block);
        } else if line.starts_with("-- @each") {
            //条件渲染
            if !temp_sql.is_empty() {
                //添加一个无条件渲染的子sql
                block_list.push(DslBlock {
                    controller: None,
                    sql: std::mem::take(&mut temp_sql),
                });
            }
            let block = parse_each(&lines, &mut i);
            block_list.push(block);
        } else if line.starts_with("-- ") {
            comment.push(line[3..].trim().to_string())
        } else {
            temp_sql.push_str(line);
            temp_sql.push('\n');
        }
        i = i + 1;
    }
    if !temp_sql.is_empty() {
        //添加一个无条件渲染的子sql
        block_list.push(DslBlock {
            controller: None,
            sql: std::mem::take(&mut temp_sql),
        });
    }

    //删除sql文中多余的空行
    block_list.iter_mut().for_each(|it| {
        it.sql = it
            .sql
            .lines()
            .map(|it| {
                if it.trim().starts_with("--") {
                    format!("\n{}\n", it)
                } else {
                    format!(" {}", it.trim())
                }
            })
            .collect::<Vec<_>>()
            .join("");
    });
    DslFunction {
        file_name,
        comment,
        name,
        return_type,
        return_list,
        return_option,
        is_page,
        params,
        param_type,
        block_list,
    }
}

/// 从某一行中解析参数信息
fn parse_dsl_param_by_line(line: &str) -> Option<DslParam> {
    //将字符串以空白字符分割
    let splits = line.split_whitespace().collect::<Vec<&str>>();

    if splits.len() < 3 {
        return None;
    }

    //得到参数名
    let mut name = splits[2].to_string();

    //参数类型
    let mut ty = "String".to_string();
    if let Some(ty_tag_start) = name.find(":") {
        //该参数名包含了参数类型
        ty = name[ty_tag_start + 1..name.len()].to_string();
        name = name[..ty_tag_start].to_string();
    }
    let is_option = if ty.ends_with("?") {
        ty = ty[..ty.len() - 1].to_string();
        true
    } else {
        false
    };
    if ty.to_lowercase().starts_with("list<"){
        ty = format!("Vec<{}", ty[5..].to_string());
    }

    //注释内容
    let mut comment = String::new();
    if splits.len() > 3 {
        comment.push_str(splits[3])
    }
    Some(DslParam {
        name,
        ty: ty.to_string(),
        comment,
        is_option,
    })
}

/// 解析某一行中的条件表达式
fn parse_if_condition(lines: &Vec<&str>, i: &mut usize) -> DslBlock {
    let mut symbols = Vec::new();
    let line = lines.get(*i).unwrap();

    //条件渲染
    let tokens = line[7..].split_whitespace().collect::<Vec<&str>>();
    let mut temp_sym: DslIfCondition = DslIfCondition::default();
    for i in 0..tokens.len() {
        let tag_index = i % 4;
        if tag_index == 0 {
            //这个是变量名
            if i > 0 {
                symbols.push(temp_sym.clone());
                temp_sym = DslIfCondition::default();
            }
            temp_sym.name = tokens[i].to_string();
        } else if tag_index == 1 {
            //这个是逻辑判断运算符,也就是:> 、<、<=、>=、== 、!=
            temp_sym.operator = tokens[i].to_string();
        } else if tag_index == 2 {
            //这个是判断值
            temp_sym.value = tokens[i].to_string();
        } else if tag_index == 3 {
            //这是与下一个判断的连接方式,也就是:&& 、||
            temp_sym.next_condition_type = tokens[i].to_string();
        } else {
        }
    }
    symbols.push(temp_sym);

    // -----------------------
    // 收集content
    // -----------------------

    let mut content = String::new();

    *i = *i + 1;
    if *i >= lines.len(){
        panic!("没有找到-- @end 标记")
    }
    while !lines[*i].starts_with("-- @end") {
        content.push_str(lines[*i]);
        *i = *i + 1;
    }
    DslBlock {
        controller: Some(DslController::If(symbols)),
        sql: content,
    }
}

/// 解析循环流程控制
pub fn parse_each(lines: &Vec<&str>, i: &mut usize) -> DslBlock {
    let first = lines[*i].trim();

    // 去掉 "-- @each"
    let text = first["-- @each ".len()..].trim();

    let mut chars = text.chars().peekable();

    // -----------------------
    // 解析 name
    // -----------------------

    let mut name = String::new();

    while let Some(c) = chars.peek() {
        if c.is_whitespace() {
            break;
        }
        name.push(*c);
        chars.next();
    }

    // 跳过空格
    while let Some(c) = chars.peek() {
        if !c.is_whitespace() {
            break;
        }
        chars.next();
    }

    let mut seq = String::new();
    let mut open = String::new();
    let mut close = String::new();
    let mut item = "item".to_string();

    // -----------------------
    // 解析 key="value"
    // -----------------------

    while chars.peek().is_some() {
        let mut key = String::new();

        while let Some(c) = chars.peek() {
            if *c == '=' {
                break;
            }
            key.push(*c);
            chars.next();
        }

        key = key.trim().to_string();

        if chars.next() != Some('=') {
            break;
        }

        if chars.next() != Some('"') {
            break;
        }

        let mut value = String::new();

        while let Some(c) = chars.next() {
            if c == '"' {
                break;
            }
            value.push(c);
        }

        match key.as_str() {
            "seq" => seq = value,
            "open" => open = value,
            "close" => close = value,
            "item" => item = value,
            _ => {}
        }

        while let Some(c) = chars.peek() {
            if !c.is_whitespace() {
                break;
            }
            chars.next();
        }
    }

    // -----------------------
    // 收集content
    // -----------------------

    let mut content = String::new();
    let mut index = *i + 1;

    while index < lines.len() {
        let line = lines[index];

        if line.trim().starts_with("-- @end") {
            break;
        }

        if !content.is_empty() {
            content.push('\n');
        }

        content.push_str(line);

        index += 1;
    }

    //修改外层行号
    *i = index;

    DslBlock {
        controller: Some(DslController::Each(DslEach {
            name,
            seq,
            open,
            close,
            item,
        })),
        sql: content,
    }
}

// 获取查询结果的Entity
// pub async fn result_entity(&self, conn: &SqlitePool) -> table::Entity {
//     let sql = self.no_condition_sql();
//     let columns = table_util::get_column_from_sql(conn, &sql).await;
//     table::Entity {
//         name: self.return_type.clone(),
//         comment: self.comment.join(","),
//         columns,
//         ..Default::default()
//     }
// }
