alter table outbound_transactions add unique_id blob;
alter table completed_transactions add unique_id blob;
alter table outputs add unique_id blob;
alter table inbound_transactions add unique_id blob;
