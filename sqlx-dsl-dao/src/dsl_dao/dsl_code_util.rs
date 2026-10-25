use crate::utils;
use crate::write_util;
use crate::dsl_dao::model::dsl_function::DslFunction;
use crate::table_dao::model::CrudType;
use sqlx::{SqliteConnection, SqlitePool};

pub async fn make_source(conn: &SqlitePool, dsl: &DslFunction) {
    let mod_name = dsl.file_name[..dsl.file_name.len() - 4].to_string();
    if dsl.crud_type() == CrudType::Read {
        if dsl.return_type != "bool" && dsl.return_type != "i64" && dsl.return_type != "String" && dsl.return_type != "chrono::NaiveDateTime" {
            /// 添加到待生成的Entity
            write_util::add_entity(&mod_name, dsl.result_entity(conn).await);
        }
    }

    //返回可选的单个标量时,需要知道该列是否可为空:可为空的列sqlx解码出来的已经是Option
    let scalar_nullable = dsl.crud_type() == CrudType::Read
        && dsl.return_option
        && !dsl.return_list
        && !dsl.is_page
        && is_scalar(&dsl.return_type)
        && dsl
            .result_entity(conn)
            .await
            .columns
            .first()
            .is_some_and(|it| it.is_nullable);

    /// 添加到待生成的代码
    write_util::add_method_source(&mod_name, make_method_src(dsl, scalar_nullable));
    if dsl.param_type.is_some() {
        /// 函数参数的Entity
        write_util::add_entity(&mod_name, dsl.param_entity());
    }
}

/// 是否是用query_scalar查询的标量类型
fn is_scalar(return_type: &str) -> bool {
    return_type == "bool"
        || return_type == "i64"
        || return_type == "String"
        || return_type == "chrono::NaiveDateTime"
}

/// 生成函数代码
/// `scalar_nullable`: 返回可选的单个标量,且该列可为空
fn make_method_src(dsl: &DslFunction, scalar_nullable: bool) -> String {
    /// sqlx查询函数:query_as_with、query_scalar...
    let mut query_func = "";

    // 返回值类型
    let mut return_type = dsl.return_type.clone();

    // sqlx绑定类型
    let mut bind_type = dsl.return_type.clone();

    // sqlx遍历数据方式
    let mut fetch_type = "fetch_one";

    // 对查询结果的转换
    let mut fetch_map = "";

    let exec_sql: String;

    // 执行sql需要的参数
    let mut exec_args = "__args".to_string();

    // 写SQL执行成功后记录写日志的代码
    let mut write_log = String::new();

    if dsl.return_option{
        return_type = format!("Option<{}>", return_type);
    }

    let template = if dsl.is_page {
        if dsl.return_type == "bool"
            || dsl.return_type == "i64"
            || dsl.return_type == "String"
        {
            query_func = "query_scalar_with::<_, [BIND_TYPE], _>"
        } else {
            query_func = "query_as_with::<_, [BIND_TYPE], _>"
        }
        exec_sql = dsl.concat_sql_src(false);

        //分页查询时
        r##"
        /// [COMMENT]
        pub async fn [FN_NAME](ctx: &mut sqlx_context::DbContext, page_in: super::PageIn, [METHOD_PARAM]) -> Result<(i64,Vec<[RETURN_TYPE]>), sqlx::Error>
        {
            let mut __sql = String::new();
            let mut __args = sqlx::sqlite::SqliteArguments::default();
            [METHOD_PARAM_AS_REF]
            [EXEC_SQL]

            let mut __sql = __sql.trim();

            //去掉sql文中的最后一个分号
            if __sql.ends_with(";"){
                __sql = &__sql[0..__sql.len() - 1];
            }

            //用来查询数据条数的sql
            let __sql_count = format!("select count(*) from ({}) as temp;", __sql);

            //设置默认页数
            let __page = if page_in.page > 0{
                page_in.page
            } else {
                1
            };

            //设置默认页面大小
            let __page_size    = if page_in.page_size > 0{
                page_in.page_size
            } else {
                10
            };
            let mut __order_by = String::new();
            if !page_in.sort_key.is_empty() {

                //验证排序方式是否非法
                page_in.check_order_by()?;
                __order_by = format!("order by {} {}", page_in.sort_key, page_in.sort_type)
            }

            //查询数据的sql
            let __sql_data = format!("select * from ({}) as temp {} limit {},{};", __sql, __order_by, (__page - 1) * __page_size, __page_size);

            //得到总数据条数
            let __count = sqlx::query_scalar_with::<_,i64,_>(&__sql_count, [ARGS].clone()).fetch_one(&mut *ctx).await?;
            if __count == 0{//没有数据,直接返回
                return Ok((0,Default::default()));
            }

            //查询到数据
            let __data = sqlx::[QUERY_FUNC](&__sql_data, [ARGS]).fetch_all(ctx).await?;
            Ok((__count, __data))
        }"##
    } else {
        match dsl.crud_type() {
            //查询数据
            CrudType::Read => {
                if dsl.return_list {
                    //如果返回值是一个列表
                    return_type = format!("Vec<{}>", return_type);
                    fetch_type = "fetch_all"
                } else if dsl.return_option {
                    //没有查到数据时返回None
                    fetch_type = "fetch_optional";
                    if scalar_nullable {
                        //列本身可为空时查询结果是Option<Option<T>>,查不到数据和值为NULL都返回None
                        fetch_map = ".map(Option::flatten)";
                    }
                }
                //如果是动态生成sql语句,只能动态拼接
                if dsl.has_if_condition() {
                    if is_scalar(&dsl.return_type) {
                        if scalar_nullable {
                            //可为空的列要按Option解码
                            bind_type = format!("Option<{}>", bind_type);
                        }
                        query_func = "query_scalar_with::<_, [BIND_TYPE], _>"
                    } else {
                        query_func = "query_as_with::<_, [BIND_TYPE], _>"
                    }
                    exec_sql = dsl.concat_sql_src(false);
                    r##"
                    /// [COMMENT]
                    pub async fn [FN_NAME](ctx: &mut sqlx_context::DbContext, [METHOD_PARAM]) -> Result<[RETURN_TYPE], sqlx::Error>
                    {
                        let mut __sql = String::new();
                        let mut __args = sqlx::sqlite::SqliteArguments::default();
                        [METHOD_PARAM_AS_REF]
                        [EXEC_SQL]
                        sqlx::[QUERY_FUNC](&__sql, [ARGS]).[FETCH_TYPE](ctx).await[FETCH_MAP]
                    }
                    "##
                } else {
                    (exec_sql, exec_args) = dsl.native_sql_and_args();
                    if is_scalar(&dsl.return_type) {
                        r##"
                        /// [COMMENT]
                        pub async fn [FN_NAME](ctx: &mut sqlx_context::DbContext, [METHOD_PARAM]) -> Result<[RETURN_TYPE], sqlx::Error>
                        {
                            [METHOD_PARAM_AS_REF]
                            sqlx::query_scalar!(r#"[EXEC_SQL]"#,[ARGS]).[FETCH_TYPE](ctx).await[FETCH_MAP]
                        }
                        "##
                    } else {
                        r##"
                        /// [COMMENT]
                        pub async fn [FN_NAME](ctx: &mut sqlx_context::DbContext, [METHOD_PARAM]) -> Result<[RETURN_TYPE], sqlx::Error>
                        {
                            [METHOD_PARAM_AS_REF]
                            sqlx::query_as!([BIND_TYPE], r#"[EXEC_SQL]"#,[ARGS]).[FETCH_TYPE](ctx).await
                        }
                        "##
                    }
                }
            }

            //修改数据
            _ => {
                fetch_type = "execute";
                //如果是动态生成sql语句,只能动态拼接
                if dsl.has_if_condition() {
                    query_func = "query_with::<_, _>";
                    exec_sql = dsl.concat_sql_src(true);
                    r##"
                    /// [COMMENT]
                    pub async fn [FN_NAME](ctx: &mut sqlx_context::DbContext, [METHOD_PARAM]) -> Result<u64, sqlx::Error>
                    {
                        let mut __sql = String::new();
                        let mut __args = sqlx::sqlite::SqliteArguments::default();

                        //开启写日志时,拼接SQL的同时收集参数
                        let __log_enabled = ctx.is_write_log_enabled();
                        let mut __log_params: Vec<sqlx_context::serde_json::Value> = Vec::new();
                        [METHOD_PARAM_AS_REF]
                        [EXEC_SQL]
                        let rs = sqlx::[QUERY_FUNC](&__sql, [ARGS]).[FETCH_TYPE](&mut *ctx).await?;
                        if __log_enabled {
                            ctx.push_write_log(__sql, __log_params);
                        }
    
                        //返回影响的行数
                        Ok(rs.rows_affected())
                    }
                    "##
                }
                //没有条件渲染,直接使用sqlx宏,提高性能
                else {
                    let arg_list;
                    (exec_sql, arg_list) = dsl.native_sql_and_arg_list();
                    exec_args = arg_list.join(", ");
                    write_log = utils::write_log_src(&format!("r#\"{}\"#", exec_sql), &arg_list);
                    r##"
                    /// [COMMENT]
                    pub async fn [FN_NAME](ctx: &mut sqlx_context::DbContext, [METHOD_PARAM]) -> Result<u64, sqlx::Error>
                    {
                        [METHOD_PARAM_AS_REF]
                        let rs = sqlx::query!(r#"[EXEC_SQL]"#, [ARGS]).[FETCH_TYPE](&mut *ctx).await?;
                        [WRITE_LOG]
    
                        //返回影响的行数
                        Ok(rs.rows_affected())
                    }
                    "##
                }
            }
        }
    };
    template
        .replace("[COMMENT]", &dsl.comment.join(";"))
        .replace("[FN_NAME]", &dsl.name)
        .replace("[METHOD_PARAM]", &dsl.method_param_src())
        .replace("[METHOD_PARAM_AS_REF]", &dsl.method_param_as_ref_src())
        .replace("[WRITE_LOG]", &write_log)
        .replace("[RETURN_TYPE]", &return_type)
        .replace("[QUERY_FUNC]", query_func)
        .replace("[BIND_TYPE]", &bind_type)
        .replace("[EXEC_SQL]", &exec_sql)
        .replace("[ARGS]", &exec_args)
        .replace("[FETCH_TYPE]", fetch_type)
        .replace("[FETCH_MAP]", fetch_map)
}
