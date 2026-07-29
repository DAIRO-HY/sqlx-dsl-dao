use crate::dsl_dao::model::dsl_block::{DslBlock, DslController};
use crate::dsl_dao::model::dsl_param::DslParam;
use crate::table_dao::model::{CrudType, table};
use crate::table_dao::table_util;
use crate::utils;
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::{HashMap, HashSet};

/// 数据操作函数
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct DslFunction {
    /// 文件名
    pub file_name: String,

    /// 注释
    pub comment: Vec<String>,

    /// 函数名
    pub name: String,

    /// 返回值
    pub return_type: String,

    /// 是否返回的是一个列表
    pub return_list: bool,

    /// 是否返回的可选类型
    pub return_option: bool,

    /// 是否分页查询
    pub is_page: bool,

    /// 参数
    pub params: Vec<DslParam>,

    /// 函数参数类型名称
    pub param_type: Option<String>,

    /// 条件渲染列表
    pub block_list: Vec<DslBlock>,
}

impl DslFunction {
    /// 该dsl中是否有条件渲染的语法
    pub fn has_if_condition(&self) -> bool {
        self.block_list.iter().any(|it| it.controller.is_some())
    }

    /// 获取非动态渲染部分的sql
    pub fn no_control_sql(&self) -> String {
        self.block_list
            .iter()
            .filter_map(|it| {
                if it.controller.is_some() {
                    return None;
                }
                Some(it.sql.clone())
            })
            .collect::<Vec<String>>()
            .join("")
    }

    /// 获取sql文的crud类型
    pub fn crud_type(&self) -> CrudType {
        let sql = self.no_control_sql().trim().to_uppercase();
        if sql.starts_with("SELECT") {
            CrudType::Read
        } else if sql.starts_with("INSERT") {
            CrudType::Create
        } else if sql.starts_with("UPDATE") {
            CrudType::Update
        } else if sql.starts_with("DELETE") {
            CrudType::Delete
        } else {
            CrudType::Unknown
        }
    }

    /// 获取查询结果的Entity
    pub async fn result_entity(&self, conn: &SqlitePool) -> table::Entity {
        let sql = self.no_control_sql();
        let columns = table_util::get_column_from_sql(conn, &sql).await;
        table::Entity {
            name: self.return_type.clone(),
            comment: self.comment.join(","),
            columns,
            ..Default::default()
        }
    }

    /// 获取函数参数实体
    pub fn param_entity(&self) -> table::Entity {
        let Some(param_type) = &self.param_type else {
            panic!("生成Dsl函数参数时,param_type必填")
        };
        let columns = self
            .params
            .iter()
            .map(|it| table::Column {
                name: it.name.clone(),
                data_type: it.rust_type().to_string(),
                comment: it.comment.clone(),
                ..Default::default()
            })
            .collect();
        table::Entity {
            name: param_type.to_string(),
            columns,
            comment: self.comment.join(","),
            ..Default::default()
        }
    }

    /// 获取函数参数部分的代码
    pub fn method_param_src(&self) -> String {
        //如果函数参数被封装到一个单独的类(结构体)时
        if let Some(param_type) = &self.param_type {
            format!("param: {}", param_type)
        } else {
            self.params.iter().fold(String::new(), |mut pre, it| {
                let rust_type = it.rust_type();
                let mut rust_type = rust_type.as_str();

                //如果函数参数是一个字符串类型,则使用字符串指针
                if rust_type == "String" {
                    rust_type = "impl AsRef<str>";
                }
                pre.push_str(it.name.as_str());
                pre.push_str(": ");
                pre.push_str(rust_type);
                pre.push_str(", ");
                pre
            })
        }
    }

    /// 生成获取字符串引用部分的代码
    pub fn method_param_as_ref_src(&self) -> String {
        //如果函数参数被封装到一个单独的类(结构体)时
        if self.param_type.is_some() {
            "".to_string()
        } else {
            self.params
                .iter()
                .filter_map(|it| {
                    if it.rust_type() == "String" {
                        return Some(format!("let {} = {}.as_ref();\n", it.name, it.name));
                    }
                    None
                })
                .collect::<Vec<String>>()
                .join("")
        }
    }

    /// sql拼接部分的代码
    pub fn concat_sql_src(&self) -> String {
        let need_param = self.param_type.is_some();
        self.block_list
            .iter()
            .map(|sql_item| sql_item.block_src(need_param))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 生成原生sql文和需要的参数代码
    pub fn native_sql_and_args(&self) -> (String, String) {
        let (native_sql, args) = table_util::parse_sql_args(self.no_control_sql().as_str());
        let var_name = if self.param_type.is_some() {
            "param."
        } else {
            ""
        };
        let exec_args = args
            .iter()
            .map(|it| format!("{}{}", var_name, it))
            .collect::<Vec<_>>()
            .join(", ");
        (native_sql, exec_args)
    }

    /// 修复一些默认配置,比如参数补足
    pub async fn fix(&self, conn: &SqlitePool) -> DslFunction {
        let mut dsl = self.fix_params(conn).await;

        //如果没有配置返回类型
        if dsl.crud_type() == CrudType::Read && dsl.return_type == "" {
            //从sql中获取返回列信息
            let columns =
                table_util::get_column_from_sql(conn, self.no_control_sql().as_str()).await;
            if columns.len() == 1 {
                let first = &columns[0];
                dsl.return_type = table_util::db_type_to_rust(&first.data_type, &first.name);
                dsl.return_option = first.is_nullable;
            } else if columns.len() > 1 {
                //返回数据列超过1列,生成一个临时的类名
                dsl.return_type = format!("{}Temp", utils::snake_to_pascal(&dsl.name, "_"))
            }
        }

        //如果没有设置函数参数类名(结构体名) 并且参数数量大于3个,则自动配置一个参数类名
        if dsl.param_type.is_none() && dsl.params.len() > 4 {
            dsl.param_type = Some(format!("{}Param", utils::snake_to_pascal(&dsl.name, "_")));
        }

        //修复参数名类名,改为驼峰命名方式
        if let Some(param_type) = dsl.param_type {
            dsl.param_type = Some(utils::snake_to_pascal(param_type, "_"));
        }

        //修复函数名
        if dsl.name.is_empty() {
            dsl.name = format!("untitled_{}", dsl.no_control_sql().len());
        }
        dsl
    }

    /// 修复函数参数,比如参数不足时,补充缺少的参数
    pub async fn fix_params(&self, conn: &SqlitePool) -> DslFunction {
        //返回的列的set
        let result_column_set =
            self.result_entity(conn)
                .await
                .columns
                .iter()
                .fold(HashMap::new(), |mut pre, it| {
                    pre.insert(it.name.clone(), it.clone());
                    pre
                });

        //得到sql文中需要的参数
        let sql_args_set = self.block_list.iter().fold(HashSet::new(), |mut pre, it| {
            let (_, args) = it.native_sql_and_args();
            let Some(controller) = &it.controller else {
                pre.extend(args);
                return pre;
            };
            match controller {
                DslController::Each(each) => args.into_iter().for_each(|it| {
                    if it != each.item && !it.starts_with(format!("{}.", &each.item).as_str()) {
                        pre.insert(it);
                    }
                }),
                _ => {
                    pre.extend(args);
                }
            }
            pre
        });

        //dsl中已经配置了的参数
        let mut current_param_set = self.params.iter().fold(HashSet::new(), |mut pre, it| {
            pre.insert(it.name.clone());
            pre
        });

        let mut params = self.params.clone();

        //修复函数参数,优先匹配返回列的列
        sql_args_set.iter().for_each(|it| {
            //该参数已经配置
            if current_param_set.contains(it) {
                return;
            }

            //从返回数据中去匹配
            if let Some(column) = result_column_set.get(it) {
                params.push(DslParam {
                    name: it.clone(),
                    ty: table_util::db_type_to_rust(&column.data_type, &column.name),
                    is_option: false,
                    ..Default::default()
                });
                current_param_set.insert(it.clone());
            }
        });

        //修复函数参数,补足没有配置的参数
        sql_args_set.iter().for_each(|it| {
            if !current_param_set.contains(it) {
                let ty = if it == "id" {
                    "i64"
                } else if it.to_lowercase().ends_with("date") {
                    "chrono::NaiveDateTime"
                }  else if it.to_lowercase().starts_with("is") {
                    "bool"
                } else {
                    "String"
                }
                .to_string();

                //该参数没有配置
                params.push(DslParam {
                    name: it.clone(),
                    ty,
                    is_option: false,
                    ..Default::default()
                })
            }
        });

        let mut fix_dsl = self.clone();
        fix_dsl.params = params;
        fix_dsl
    }
}
