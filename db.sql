CREATE TABLE IF NOT EXISTS "feed"
(
    id           integer                            not null
        primary key,
    title        varchar(255)                       not null,
    last_updated datetime default CURRENT_TIMESTAMP not null
);
CREATE TABLE IF NOT EXISTS "feed_url"
(
    id      integer      not null
        primary key,
    feed_id integer      not null
        references feed,
    url     varchar(255) not null
        constraint uniq_url
            unique
);
CREATE TABLE IF NOT EXISTS "item"
(
    id          integer                            not null
        primary key,
    feed_id     integer                            not null
        references feed,
    guid        varchar(255)                       not null,
    title       varchar(255)                       not null,
    author      varchar(255)                       not null,
    url         varchar(255)                       not null,
    content     text                               not null,
    is_saved    integer  default 0                 not null,
    is_read     integer  default 0                 not null,
    create_time datetime default CURRENT_TIMESTAMP not null,
    constraint uniq_feed_id_guid
        unique (feed_id, guid)
);
