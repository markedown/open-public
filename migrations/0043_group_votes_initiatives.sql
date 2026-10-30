-- What parliamentary groups did in the chamber beyond recorded votes.
--
-- Most votes are by show of hands. They have no list of names, but the record of
-- the sitting says which groups voted which way, so the vote is held per party with
-- the record's own wording next to it. `side` is relative to the subject: for means
-- for the bill or motion, even where the chamber voted on a recommendation to reject
-- it. A party in a joint group is listed under that group's vote.
--
-- An initiative is a bill or motion a party or the government tabled: the act open
-- to a party that cannot legislate.
--
-- Migrations are append-only; never edit this file once it has been applied.

create table initiatives (
    id bigserial primary key,
    country_id bigint not null references countries (id),
    slug text not null unique,
    kind text not null check (kind in ('bill', 'motion')),
    title text not null,
    document text not null,
    tabled_on date not null,
    -- The chamber's own word for where it stands, unchanged.
    status text,
    by_government boolean not null default false,
    topics text[] not null default '{}',
    source_id bigint not null references sources (id)
);

create index initiatives_country_tabled_idx on initiatives (country_id, tabled_on desc);

create table initiative_parties (
    initiative_id bigint not null references initiatives (id) on delete cascade,
    party_id bigint not null references parties (id),
    primary key (initiative_id, party_id)
);

create index initiative_parties_party_idx on initiative_parties (party_id);

create table group_votes (
    id bigserial primary key,
    country_id bigint not null references countries (id),
    slug text not null unique,
    chamber text not null,
    held_on date not null,
    title text not null,
    document text,
    initiative_id bigint references initiatives (id) on delete set null,
    adopted boolean,
    -- The record's wording, verbatim, so the reading can be checked on the page.
    excerpt text not null,
    locator text,
    source_id bigint not null references sources (id)
);

create index group_votes_country_held_idx on group_votes (country_id, held_on desc);

create table group_vote_parties (
    group_vote_id bigint not null references group_votes (id) on delete cascade,
    party_id bigint not null references parties (id),
    side text not null check (side in ('for', 'against', 'abstain')),
    primary key (group_vote_id, party_id)
);

create index group_vote_parties_party_idx on group_vote_parties (party_id);

-- A motion tabled by a party is an act of its own, not the same thing as a bill.
alter table position_evidence drop constraint position_evidence_kind_check;
alter table position_evidence add constraint position_evidence_kind_check
    check (kind = any (array['manifesto', 'statement', 'bill', 'motion', 'court', 'vote',
                             'law', 'decree', 'alliance']));
