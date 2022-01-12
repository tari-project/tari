-- alter table outbound_transactions add unique_id blob;
-- alter table completed_transactions add unique_id blob;
alter table outputs add metadata blob;
alter table outputs add features_parent_public_key blob;
alter table outputs add features_unique_id blob;
