
CREATE VIRTUAL TABLE IF NOT EXISTS post_search USING fts5(id UNINDEXED,title,body);
CREATE TRIGGER IF NOT EXISTS post_search_insert AFTER INSERT ON posts WHEN new.status='published' BEGIN
 INSERT INTO post_search(id,title,body) VALUES(new.id,new.published_title,new.published_body); END;
CREATE TRIGGER IF NOT EXISTS post_search_update AFTER UPDATE OF status,published_title,published_body ON posts WHEN old.status<>new.status OR old.published_title<>new.published_title OR old.published_body<>new.published_body BEGIN
 DELETE FROM post_search WHERE id=old.id;
 INSERT INTO post_search(id,title,body) SELECT new.id,new.published_title,new.published_body WHERE new.status='published'; END;
CREATE TRIGGER IF NOT EXISTS post_search_delete AFTER DELETE ON posts BEGIN DELETE FROM post_search WHERE id=old.id; END;
