table! {
    contacts (pub_key) {
        pub_key -> Text,
        screen_name -> Text,
        address -> Text,
    }
}

table! {
    received_messages (id) {
        id -> Binary,
        source_pub_key -> Text,
        dest_pub_key -> Text,
        message -> Text,
        timestamp -> Timestamp,
    }
}

table! {
    sent_messages (id) {
        id -> Text,
        source_pub_key -> Text,
        dest_pub_key -> Text,
        message -> Text,
        timestamp -> Timestamp,
        acknowledged -> Integer,
        is_read -> Integer,
    }
}

table! {
    settings (pub_key) {
        pub_key -> Text,
        screen_name -> Text,
    }
}

joinable!(sent_messages -> contacts (dest_pub_key));

allow_tables_to_appear_in_same_query!(contacts, received_messages, sent_messages, settings,);
