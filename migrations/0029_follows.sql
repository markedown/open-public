-- A signed-in user can follow entities (a person, party, country or topic) to
-- build a personal feed. Polymorphic by design, so entity_id carries no single
-- foreign key; the entity kinds are constrained instead. No user-to-user graph.
create table follows (
    id bigserial primary key,
    user_id bigint not null references users (id),
    entity_type text not null check (entity_type in ('person', 'party', 'country', 'topic')),
    entity_id bigint not null,
    created_at timestamptz not null default now(),
    unique (user_id, entity_type, entity_id)
);

create index follows_user_idx on follows (user_id);
