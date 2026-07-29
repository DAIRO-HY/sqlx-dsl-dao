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

    /// 添加到待生成的代码
    write_util::add_method_source(&mod_name, make_method_src(dsl));
    if dsl.param_type.is_some() {
        /// 函数参数的Entity
        write_util::add_entity(&mod_name, dsl.param_entity());
    }
}

/// 生成函数代码
fn make_method_src(dsl: &DslFunction) -> String {
    /// sqlx查询函数:query_as_with、query_scalar...
    let mut query_func = "";

    // 返回值类型
    let mut return_type = dsl.return_type.clone();

    // sqlx绑定类型
    let bind_type = dsl.return_type.clone();

    // sqlx遍历数据方式
    let mut fetch_type = "fetch_one";

    let exec_sql: String;

    // 执行sql需要的参数
    let mut exec_args = "args".to_string();

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
        exec_sql = dsl.concat_sql_src();

        //分页查询时
        r##"
        /// [COMMENT]
        pub async fn [FN_NAME](ctx: &mut sqlx_context::DbContext, page_in: super::PageIn, [METHOD_PARAM]) -> Result<(i64,Vec<[RETURN_TYPE]>), sqlx::Error>
        {
            let mut sql = String::new();
            let mut args = sqlx::sqlite::SqliteArguments::default();
            [METHOD_PARAM_AS_REF]
            [EXEC_SQL]

            let mut sql = sql.trim();

            //去掉sql文中的最后一个分号
            if sql.ends_with(";"){
                sql = &sql[0..sql.len() - 1];
            }

            //用来查询数据条数的sql
            let sql_count = format!("select count(*) from ({}) as temp;", sql);

            //设置默认页数
            let page = if page_in.page > 0{
                page_in.page
            } else {
                1
            };

            //设置默认页面大小
            let page_size    = if page_in.page_size > 0{
                page_in.page_size
            } else {
                10
            };
            let mut order_by = String::new();
            if !page_in.sort_key.is_empty() {

                //验证排序方式是否非法
                page_in.check_order_by()?;
                order_by = format!("order by {} {}", page_in.sort_key, page_in.sort_type)
            }

            //查询数据的sql
            let sql_data = format!("select * from ({}) as temp {} limit {},{};", sql, order_by, (page - 1) * page_size, page_size);

            //得到总数据条数
            let count = sqlx::query_scalar_with::<_,i64,_>(&sql_count, [ARGS].clone()).fetch_one(&mut *ctx).await?;
            if count == 0{//没有数据,直接返回
                return Ok((0,Default::default()));
            }

            //查询到数据
            let data = sqlx::[QUERY_FUNC](&sql_data, [ARGS]).fetch_all(ctx).await?;
            Ok((count, data))
        }"##
    } else {
        match dsl.crud_type() {
            //查询数据
            CrudType::Read => {
                if dsl.return_list {
                    //如果返回值是一个列表
                    return_type = format!("Vec<{}>", return_type);
                    fetch_type = "fetch_all"
                }
                //如果是动态生成sql语句,只能动态拼接
                if dsl.has_if_condition() {
                    if dsl.return_type == "bool"
                        || dsl.return_type == "i64"
                        || dsl.return_type == "String"
                        || dsl.return_type == "chrono::NaiveDateTime"
                    {
                        query_func = "query_scalar_with::<_, [BIND_TYPE], _>"
                    } else {
                        query_func = "query_as_with::<_, [BIND_TYPE], _>"
                    }
                    exec_sql = dsl.concat_sql_src();
                    r##"
                    /// [COMMENT]
                    pub async fn [FN_NAME](ctx: &mut sqlx_context::DbContext, [METHOD_PARAM]) -> Result<[RETURN_TYPE], sqlx::Error>
                    {
                        let mut sql = String::new();
                        let mut args = sqlx::sqlite::SqliteArguments::default();
                        [METHOD_PARAM_AS_REF]
                        [EXEC_SQL]
                        sqlx::[QUERY_FUNC](&sql, [ARGS]).[FETCH_TYPE](ctx).await
                    }
                    "##
                } else {
                    (exec_sql, exec_args) = dsl.native_sql_and_args();
                    if dsl.return_type == "bool"
                        || dsl.return_type == "i64"
                        || dsl.return_type == "String"
                        || dsl.return_type == "chrono::NaiveDateTime"
                    {
                        r##"
                        /// [COMMENT]
                        pub async fn [FN_NAME](ctx: &mut sqlx_context::DbContext, [METHOD_PARAM]) -> Result<[RETURN_TYPE], sqlx::Error>
                        {
                            [METHOD_PARAM_AS_REF]
                            sqlx::query_scalar!(r#"[EXEC_SQL]"#,[ARGS]).[FETCH_TYPE](ctx).await
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
                    exec_sql = dsl.concat_sql_src();
                    r##"
                    /// [COMMENT]
                    pub async fn [FN_NAME](ctx: &mut sqlx_context::DbContext, [METHOD_PARAM]) -> Result<u64, sqlx::Error>
                    {
                        let mut sql = String::new();
                        let mut args = sqlx::sqlite::SqliteArguments::default();
                        [METHOD_PARAM_AS_REF]
                        [EXEC_SQL]
                        let rs = sqlx::[QUERY_FUNC](&sql, [ARGS]).[FETCH_TYPE](ctx).await?;
    
                        //返回影响的行数
                        Ok(rs.rows_affected())
                    }
                    "##
                }
                //没有条件渲染,直接使用sqlx宏,提高性能
                else {
                    (exec_sql, exec_args) = dsl.native_sql_and_args();
                    r##"
                    /// [COMMENT]
                    pub async fn [FN_NAME](ctx: &mut sqlx_context::DbContext, [METHOD_PARAM]) -> Result<u64, sqlx::Error>
                    {
                        [METHOD_PARAM_AS_REF]
                        let rs = sqlx::query!(r#"[EXEC_SQL]"#, [ARGS]).[FETCH_TYPE](ctx).await?;
    
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
        .replace("[RETURN_TYPE]", &return_type)
        .replace("[QUERY_FUNC]", query_func)
        .replace("[BIND_TYPE]", &bind_type)
        .replace("[EXEC_SQL]", &exec_sql)
        .replace("[ARGS]", &exec_args)
        .replace("[FETCH_TYPE]", fetch_type)
}
