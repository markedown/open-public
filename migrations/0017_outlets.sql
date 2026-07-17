-- News outlets as first-class entities.
--
-- An outlet is the organization that publishes an article. Until now the outlet
-- was free text on `sources.outlet`; this promotes it to its own row so an
-- outlet can carry a logo, a homepage, a neutral summary and a political-leaning
-- assessment, and so the articles saved from it can be listed on its own page.
--
-- Political leaning is a fact like any other: it references the source that
-- assessed it (`leaning_source_id`), and it uses a country-neutral five-point
-- spectrum. NULL leaning means unassessed.
--
-- Migrations are append-only; never edit this file once it has been applied.

create table outlets (
    id                bigserial primary key,
    name              text not null,
    slug              text not null unique,
    homepage_url      text,
    logo_url          text,
    logo_license      text,
    leaning           text check (leaning in
                          ('left', 'lean_left', 'center', 'lean_right', 'right')),
    leaning_source_id bigint references sources (id),
    summary           text,
    source_id         bigint not null references sources (id),
    created_at        timestamptz not null default now(),
    updated_at        timestamptz not null default now()
);

-- Every source (article) may name the outlet that published it.
alter table sources add column outlet_id bigint references outlets (id);
create index sources_outlet_id_idx on sources (outlet_id);
