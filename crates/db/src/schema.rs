// @generated automatically by Diesel CLI.

diesel::table! {
    accounts (id) {
        id -> Text,
        position -> Nullable<Integer>,
        color -> Nullable<Text>,
        alias -> Nullable<Text>,
        disabled -> Bool,
        hidden -> Bool,
    }
}

diesel::table! {
    app_state (id) {
        id -> Integer,
        first_day_of_week -> Integer,
        calendar_view -> Text,
    }
}

diesel::table! {
    calendars (id) {
        id -> Text,
        disabled -> Bool,
    }
}

diesel::table! {
    folded_sections (section) {
        section -> Text,
    }
}

diesel::allow_tables_to_appear_in_same_query!(accounts, app_state, calendars, folded_sections,);
