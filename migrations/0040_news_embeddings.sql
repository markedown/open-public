-- News similarity and related-coverage clustering.
--
-- pgvector holds an embedding of each article's headline and summary, so
-- same-story coverage across outlets can be found by nearest-neighbour search
-- (the "also reported by" spread). The related links are stored once per pair,
-- in canonical order, with how they were matched, so a page shows related
-- coverage without recomputing it. Fetch provenance (when read, the hash of what
-- was read) already lives on the source row, so no new column is needed there.

create extension if not exists vector;

alter table news_items add column embedding vector(1024);

-- Approximate nearest-neighbour over cosine distance, for candidate matches.
create index news_items_embedding_idx
    on news_items using hnsw (embedding vector_cosine_ops);

create table news_related (
    a_id       bigint not null references news_items (id) on delete cascade,
    b_id       bigint not null references news_items (id) on delete cascade,
    method     text not null,        -- how matched: 'embedding', 'verified', ...
    similarity real,                 -- cosine similarity when method = 'embedding'
    created_at timestamptz not null default now(),
    primary key (a_id, b_id),
    -- Symmetric: stored once, in a canonical order, so a pair cannot appear twice.
    check (a_id < b_id)
);

-- The primary key indexes lookups by a_id; this covers lookups by b_id.
create index news_related_b_idx on news_related (b_id);
