-- Verifiable vote integrity: a per-poll append-only hash chain.
--
-- Each vote is chained within its poll:
--   row_hash = sha256( prev_hash || content )
--   content  = big-endian( poll_id, seq, option_id, voter_index, cast_at_micros )
-- The genesis (seq 1) uses a poll-specific prev_hash so chains cannot be spliced
-- between polls. Any retroactive edit changes that row's hash and every later
-- one, so a recomputed chain head no longer matches the published head. This
-- proves votes were not altered, reordered, inserted, or removed after casting.
-- It does NOT prove one-person-one-vote; that is a later phase, and the informal
-- label stays.
--
-- Enforced in the database (BEFORE/AFTER triggers using pgcrypto), so no insert
-- path (application, admin, or direct SQL) can add an unchained vote. voter_index
-- is an opaque per-poll counter, never the user id, so published dumps never leak
-- voter identity.
--
-- Determinism contract (the verifier must match exactly): integers are 8-byte
-- big-endian (int8send); cast_at_micros = (extract(epoch from cast_at) * 1e6)
-- rounded to a bigint; prev_hash and row_hash are 32-byte SHA-256 digests.
--
-- Migrations are append-only; never edit this file once applied.

create extension if not exists pgcrypto;

-- Poll-specific genesis hash.
create function vote_chain_genesis(p_poll_id bigint) returns bytea
    language sql immutable as $$
    select digest('open-public/poll/' || p_poll_id::text, 'sha256')
$$;

-- The row hash for a single vote.
create function vote_chain_hash(
    p_prev bytea,
    p_poll_id bigint,
    p_seq bigint,
    p_option_id bigint,
    p_voter_index bigint,
    p_cast_at timestamptz
) returns bytea language sql immutable as $$
    select digest(
        p_prev
        || int8send(p_poll_id)
        || int8send(p_seq)
        || int8send(p_option_id)
        || int8send(p_voter_index)
        || int8send((extract(epoch from p_cast_at) * 1000000)::bigint),
        'sha256'
    )
$$;

-- Per-poll chain head: serializes concurrent writes and exposes the fingerprint.
create table poll_chains (
    poll_id          bigint primary key references polls(id),
    head_seq         bigint not null default 0,
    head_hash        bytea  not null,
    next_voter_index bigint not null default 1
);

alter table poll_votes
    add column seq         bigint,
    add column voter_index bigint,
    add column row_hash    bytea;

-- Backfill any pre-existing votes, chaining each poll in (cast_at, id) order.
-- The append-only guard is briefly disabled for this one-time migration.
alter table poll_votes disable trigger poll_votes_block_update;
do $$
declare
    p record;
    v record;
    s bigint;
    idx bigint;
    prev bytea;
    h bytea;
begin
    for p in (select distinct poll_id as pid from poll_votes) loop
        s := 0;
        idx := 0;
        prev := vote_chain_genesis(p.pid);
        for v in (
            select id, poll_id, option_id, cast_at
            from poll_votes where poll_id = p.pid order by cast_at, id
        ) loop
            s := s + 1;
            idx := idx + 1;
            h := vote_chain_hash(prev, v.poll_id, s, v.option_id, idx, v.cast_at);
            update poll_votes set seq = s, voter_index = idx, row_hash = h where id = v.id;
            prev := h;
        end loop;
        insert into poll_chains(poll_id, head_seq, head_hash, next_voter_index)
            values (p.pid, s, prev, idx + 1);
    end loop;
end $$;
alter table poll_votes enable trigger poll_votes_block_update;

alter table poll_votes
    alter column seq set not null,
    alter column voter_index set not null,
    alter column row_hash set not null,
    add constraint poll_votes_poll_seq_key unique (poll_id, seq),
    add constraint poll_votes_poll_voter_key unique (poll_id, voter_index);

-- On insert, fill seq/voter_index/row_hash and advance the head, all in one
-- BEFORE trigger. Everything is done here (not in an AFTER trigger) because
-- AFTER-row triggers are deferred to statement end, so a multi-row insert would
-- otherwise assign every row the same seq. Order matters: lock the head first to
-- serialize concurrent votes, then reject a repeat vote (so a skipped vote never
-- advances the head), then chain and advance.
create function poll_votes_chain() returns trigger language plpgsql as $$
declare
    h_seq bigint;
    h_hash bytea;
    n_idx bigint;
begin
    insert into poll_chains(poll_id, head_hash)
        values (new.poll_id, vote_chain_genesis(new.poll_id))
        on conflict (poll_id) do nothing;
    select head_seq, head_hash, next_voter_index
        into h_seq, h_hash, n_idx
        from poll_chains where poll_id = new.poll_id for update;

    -- A repeat vote by the same user is skipped and leaves the head untouched
    -- (the unique constraint on (poll_id, user_id) is the backstop).
    if exists (
        select 1 from poll_votes where poll_id = new.poll_id and user_id = new.user_id
    ) then
        return null;
    end if;

    new.seq := h_seq + 1;
    new.voter_index := n_idx;
    new.row_hash := vote_chain_hash(
        h_hash, new.poll_id, new.seq, new.option_id, new.voter_index, new.cast_at
    );

    update poll_chains set
        head_seq = new.seq,
        head_hash = new.row_hash,
        next_voter_index = n_idx + 1
    where poll_id = new.poll_id;
    return new;
end $$;

create trigger poll_votes_chain_before
    before insert on poll_votes
    for each row execute function poll_votes_chain();
