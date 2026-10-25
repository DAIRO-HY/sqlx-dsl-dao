-- @name set_delete_date_by_ids
-- @param ids:List<i64> 文件id列表
-- @param deleteDate:i64 删除日期
update dfs_file set deleteDate = :deleteDate where id in
-- @each ids seq="," open="(" close=")" item="id"
:id
-- @end

-- ------------------------------

-- @name update_state
-- @param id:i64 文件id
-- @param state:i64 处理状态
-- @param stateMsg:String? 出错信息,为空则不更新
update dfs_file set state = :state
-- @if stateMsg != null
, stateMsg = :stateMsg
-- @end
where id = :id

-- ------------------------------

-- 未声明参数类型:deleteDate 与表字段同名,应推断为 i64 而不是 NaiveDateTime
-- @name recover
UPDATE dfs_file SET deleteDate = :deleteDate WHERE id = :id

-- ------------------------------

-- @name move_to_delete
-- @param id:i64 文件id
insert into dfs_file_delete select * from dfs_file where id = :id

-- ------------------------------

-- @name select_by_parent
-- @param parentId:i64 父目录id
-- @return List<DfsFile>
select * from dfs_file where parentId = :parentId and deleteDate is null
