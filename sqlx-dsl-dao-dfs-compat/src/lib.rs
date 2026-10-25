//! 用 DairoDFS 的建表语句验证 sqlx-dsl-dao 的兼容性,只包含测试。

include!(concat!(env!("OUT_DIR"), "/dao.rs"));

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx_context::DbContext;

    /// 和 build.rs 一样,用 sql/create/ 下的原始DDL建一个内存数据库
    async fn memory_ctx() -> DbContext {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for sql in [
            include_str!("../sql/create/dfs_file.sql"),
            include_str!("../sql/create/storage_file.sql"),
            include_str!("../sql/create/share.sql"),
            include_str!("../sql/create/sql_log.sql"),
            include_str!("../sql/create/user.sql"),
            include_str!("../sql/create/user_token.sql"),
        ] {
            sqlx::raw_sql(sql).execute(&pool).await.unwrap();
        }
        let sql = include_str!("../sql/create/dfs_file.sql").replace("dfs_file", "dfs_file_delete");
        let _ = sqlx::raw_sql(&sql).execute(&pool).await;
        DbContext::new(pool)
    }

    /// 模拟 Go 节点重放时的判断:去掉空白后区分大小写比较前缀
    fn go_need_download(sql: &str) -> bool {
        let sql: String = sql.chars().filter(|it| !it.is_whitespace()).collect();
        sql.starts_with("insertintodfs_file(") || sql.starts_with("insertintostorage_file")
    }

    fn dfs_file(id: i64) -> dfs_file_dao::DfsFile {
        dfs_file_dao::DfsFile {
            id,
            userId: 1,
            parentId: 0,
            name: "a.mp4".to_string(),
            ext: "mp4".to_string(),
            size: 1024,
            contentType: Some("video/mp4".to_string()),
            storageId: 7,
            date: 1700000000000,
            property: None,
            isExtra: 0,
            isHistory: 0,
            deleteDate: None,
            state: 0,
            stateMsg: None,
        }
    }

    #[test]
    fn column_types_follow_ddl() {
        let f = dfs_file(1);
        // INT8 毫秒时间戳必须是 i64,不能因为列名以 date 结尾变成 NaiveDateTime
        let _: i64 = f.date;
        let _: Option<i64> = f.deleteDate;
        let _: i64 = f.isExtra; // INTEGER
        let _: Option<String> = f.property; // TEXT NULL
        let _: String = f.name; // VARCHAR(256) COLLATE NOCASE NOT NULL
        let s = share_dao::Share::default();
        let _: Option<i64> = s.endDate;
        let _: Option<i64> = s.folderFlag; // INTEGER,-- 注释紧贴逗号
        let _: i64 = s.fileCount;
        let u = user_dao::User::default();
        let _: Option<String> = u.name; // NULL unique
        let _: Option<String> = u.pwd; // CHAR(32)
        let _: i64 = u.state; // INTEGER NOT NULL default 1
        let l = sql_log_dao::SqlLog::default();
        let _: String = l.sql; // 小写 text
        let _: Option<String> = l.err;
        let t = user_token_dao::UserToken::default();
        let _: i64 = t.version;
    }

    #[async_std::test]
    async fn dfs_file_insert_log_is_recognized_by_go_node() {
        let mut ctx = memory_ctx().await;
        ctx.enable_write_log();
        dfs_file_dao::insert(&mut ctx, &dfs_file(1)).await.unwrap();
        storage_file_dao::insert(
            &mut ctx,
            &storage_file_dao::StorageFile {
                id: 7,
                path: "/data/7".to_string(),
                md5: "d41d8cd98f00b204e9800998ecf8427e".to_string(),
            },
        )
        .await
        .unwrap();

        let logs = ctx.take_write_log();
        assert_eq!(
            logs[0].sql,
            "insert into dfs_file (id, userId, parentId, name, ext, size, contentType, storageId, date, property, isExtra, isHistory, deleteDate, state, stateMsg) values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        );
        assert_eq!(
            logs[0].params,
            [
                json!(1), json!(1), json!(0), json!("a.mp4"), json!("mp4"), json!(1024),
                json!("video/mp4"), json!(7), json!(1700000000000i64), json!(null), json!(0),
                json!(0), json!(null), json!(0), json!(null),
            ]
        );
        assert_eq!(
            logs[0].params_json(),
            r#"[1,1,0,"a.mp4","mp4",1024,"video/mp4",7,1700000000000,null,0,0,null,0,null]"#
        );
        assert_eq!(logs[1].sql, "insert into storage_file (id, path, md5) values (?, ?, ?)");
        assert!(logs.iter().all(|it| go_need_download(&it.sql)));
    }

    #[async_std::test]
    async fn user_token_version_is_a_plain_column() {
        let mut ctx = memory_ctx().await;
        ctx.enable_write_log();
        let mut token = user_token_dao::UserToken {
            id: 1,
            token: "t".to_string(),
            userId: 1,
            clientFlag: 0,
            deviceId: "d".to_string(),
            ip: "127.0.0.1".to_string(),
            date: 1700000000000,
            version: 10203,
        };
        user_token_dao::insert(&mut ctx, &token).await.unwrap();
        token.version = 10204;
        assert_eq!(user_token_dao::update(&mut ctx, &token).await.unwrap(), 1);
        assert_eq!(user_token_dao::select_one(&mut ctx, 1).await.unwrap().version, 10204);

        let logs = ctx.take_write_log();
        assert_eq!(
            logs[0].sql,
            "insert into user_token (id, token, userId, clientFlag, deviceId, ip, date, version) values (?, ?, ?, ?, ?, ?, ?, ?)"
        );
        assert_eq!(
            logs[1].sql,
            "update user_token set token = ?, userId = ?, clientFlag = ?, deviceId = ?, ip = ?, date = ?, version = ? where id = ?"
        );
        assert_eq!(logs[1].params[6], json!(10204));
    }

    #[async_std::test]
    async fn dsl_write_log_keeps_sql_as_written() {
        let mut ctx = memory_ctx().await;
        dfs_file_dao::insert(&mut ctx, &dfs_file(1)).await.unwrap();
        dfs_file_dao::insert(&mut ctx, &dfs_file_dao::DfsFile { name: "b.mp4".into(), ..dfs_file(2) })
            .await
            .unwrap();
        ctx.enable_write_log();

        assert_eq!(dfs_file_dao::set_delete_date_by_ids(&mut ctx, vec![1, 2], 99).await.unwrap(), 2);
        dfs_file_dao::update_state(&mut ctx, 1, 2, Some("err".to_string())).await.unwrap();
        dfs_file_dao::update_state(&mut ctx, 1, 1, None).await.unwrap();
        dfs_file_dao::recover(&mut ctx, 0, 2).await.unwrap();
        dfs_file_dao::move_to_delete(&mut ctx, 1).await.unwrap();
        assert_eq!(dfs_file_dao::select_by_parent(&mut ctx, 0).await.unwrap().len(), 0);

        let logs = ctx.take_write_log();
        let logs: Vec<(&str, String)> = logs.iter().map(|it| (it.sql.as_str(), it.params_json())).collect();
        assert_eq!(
            logs,
            [
                ("update dfs_file set deleteDate = ? where id in (?,?) ", "[99,1,2]".to_string()),
                ("update dfs_file set state = ? , stateMsg = ? where id = ? ", r#"[2,"err",1]"#.to_string()),
                ("update dfs_file set state = ? where id = ? ", "[1,1]".to_string()),
                ("UPDATE dfs_file SET deleteDate = ? WHERE id = ?", "[0,2]".to_string()),
                ("insert into dfs_file_delete select * from dfs_file where id = ?", "[1]".to_string()),
            ]
        );
        // insert into dfs_file_delete 不会被 Go 节点误判为需要下载文件
        assert!(!go_need_download(logs[4].0));
        assert_eq!(dfs_file_delete_dao::select_one(&mut ctx, 1).await.unwrap().deleteDate, Some(99));
    }

    #[async_std::test]
    async fn unique_index_with_where_still_applies() {
        let mut ctx = memory_ctx().await;
        let file = dfs_file_dao::DfsFile { parentId: 5, ..dfs_file(1) };
        dfs_file_dao::insert(&mut ctx, &file).await.unwrap();
        // idx_name:同一文件夹下文件名不允许重复(name 比较忽略大小写)
        let dup = dfs_file_dao::DfsFile { name: "A.MP4".into(), ..dfs_file_dao::DfsFile { id: 2, ..file } };
        assert!(dfs_file_dao::insert(&mut ctx, &dup).await.is_err());
    }
}
