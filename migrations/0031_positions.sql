-- The positions layer for the preference-match compass. `theses` are curated,
-- sourced policy statements per country; `party_positions` record each party's
-- stance on each thesis on a five-point scale (-2 strongly disagree .. +2
-- strongly agree). Every fact carries a source, per the content rules. Visitor
-- answers are never stored: the match is computed statelessly.
create table theses (
    id bigserial primary key,
    country_id bigint not null references countries (id),
    topic_id bigint references topics (id),
    text text not null,
    position int not null default 0,
    source_id bigint not null references sources (id),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);
create index theses_country_idx on theses (country_id, position);

create table party_positions (
    id bigserial primary key,
    thesis_id bigint not null references theses (id) on delete cascade,
    party_id bigint not null references parties (id),
    stance smallint not null check (stance between -2 and 2),
    justification text,
    source_id bigint not null references sources (id),
    unique (thesis_id, party_id)
);
create index party_positions_thesis_idx on party_positions (thesis_id);
