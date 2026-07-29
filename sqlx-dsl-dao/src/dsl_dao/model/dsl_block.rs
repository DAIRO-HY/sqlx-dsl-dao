use crate::dsl_dao::model::dsl_each::DslEach;
use crate::dsl_dao::model::dsl_if_condition::DslIfCondition;
use crate::table_dao::table_util;

#[derive(Debug, Default, Clone, serde::Serialize)]
pub enum DslController {
    /// if流程控制
    If(Vec<DslIfCondition>),

    /// for循环控制
    Each(DslEach),

    #[default]
    None,
}

/// 条件渲染Sql部分信息
#[derive(Debug, serde::Serialize, Default, Clone)]
pub struct DslBlock {
    /// 流程控制
    pub controller: Option<DslController>, //DSL中的条件渲染表达式列表

    /// 条件渲染部分的sql
    pub sql: String, //条件渲染部分的sql
}

impl DslBlock {
    /// 获取原生sql文,和sql参数
    pub fn native_sql_and_args(&self) -> (String, Vec<String>) {
        table_util::parse_sql_args(&self.sql)
    }

    /// 获取添加参?传值的代码
    pub fn block_src(&self, need_param: bool) -> String {
        let (native_sql, args) = table_util::parse_sql_args(&self.sql);
        let var_name = if need_param { "param." } else { "" };

        let mut body = String::new();

        //没有流程控制
        let Some(controller) = &self.controller else {
            let add_args_src = args.iter().fold(String::new(), |mut pre, it| {
                let add_arg_src = format!(
                    "sqlx::Arguments::add(&mut args, &{}{}).unwrap();\n",
                    var_name, it
                );
                pre.push_str(add_arg_src.as_str());
                pre
            });
            body.push_str(format!("sql.push_str(r#\"{} \"#);\n", native_sql.trim()).as_str());
            body.push_str(&add_args_src);
            return body;
        };

        //这是一个带渲染条件的sql
        match controller {
            // if流程控制
            DslController::If(conditions) => {
                let add_args_src = args.iter().fold(String::new(), |mut pre, it| {
                    let add_arg_src = format!(
                        "sqlx::Arguments::add(&mut args, &{}{}).unwrap();\n",
                        var_name, it
                    );
                    pre.push_str(add_arg_src.as_str());
                    pre
                });
                body.push_str("if ");

                //if条件代码
                let if_condition_src = conditions
                    .iter()
                    .map(|it| it.condition_src(need_param))
                    .collect::<Vec<_>>()
                    .join("");
                r##"
                    if [CONDITION] {
                        sql.push_str(r#"[NATIVE_SQL] "#);
                        [ADD_ARGS]
                    }
                "##
                .replace("[CONDITION]", &if_condition_src)
                .replace("[NATIVE_SQL]", native_sql.trim())
                .replace("[ADD_ARGS]", &add_args_src)
            }
            DslController::Each(each) => {
                //要循环的变量名
                let name = format!("{}{}", var_name, each.name);
                let add_args_src = args.iter().fold(String::new(), |mut pre, it| {
                    let add_arg_src = if it == &each.item
                        || it.starts_with(format!("{}.", &each.item).as_str())
                    {
                        //each循环不能在参数前加&,且不能加函数参数前缀
                        format!("sqlx::Arguments::add(&mut args, {}).unwrap();\n", it)
                    } else {
                        format!(
                            "sqlx::Arguments::add(&mut args, &{}{}).unwrap();\n",
                            var_name, it
                        )
                    };
                    pre.push_str(add_arg_src.as_str());
                    pre
                });

                //删除最后一个分割字符串
                let truncate_seq = if each.seq.len() > 0 {
                    format!("//去掉最后一段分隔符\nsql.truncate(sql.len() - {});", each.seq.len())
                }else{
                    "".to_string()
                };
                r##"
                    if [NAME].len() > 0 {
                         sql.push_str("[OPEN]");
                         [NAME].iter().for_each(|[ITEM]|{
                            sql.push_str(r#"[NATIVE_SQL][SEQ]"#);
                            [ADD_ARGS]
                         });
                         [TRUNCATE_SEQ]
                         sql.push_str("[CLOSE]");
                         sql.push_str(" ");
                     }
                "##
                .replace("[NAME]", &name)
                .replace("[OPEN]", &each.open)
                .replace("[ITEM]", &each.item)
                .replace("[NATIVE_SQL]", native_sql.trim())
                .replace("[SEQ]", &each.seq)
                .replace("[ADD_ARGS]", &add_args_src)
                .replace("[TRUNCATE_SEQ]", &truncate_seq)
                .replace("[CLOSE]", &each.close)
            }
            DslController::None => "".to_string(),
        }
    }
}
