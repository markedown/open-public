-- Multi-select polls: a voter may pick several options in one poll.
--
-- This changes the one-vote-per-poll invariant and the integrity chain, so it
-- gets its own migration and a rewritten trigger.
--
-- Model:
--   * single/yesno/scale  one vote row per (poll, user)         (unchanged)
--   * multi               one vote row per selected option, i.e.
--                         several rows per (poll, user)
--
-- The append-only hash chain is preserved unchanged in shape: each vote row is
-- still one chain link (its own seq and row_hash), so the published head and the
-- independent verifier keep working with no formula change. What changes:
--   * the (poll_id, user_id) uniqueness is replaced by (poll_id, user_id,
--     option_id): a voter cannot pick the SAME option twice, but can pick
--     several distinct options in a multi poll;
--   * voter_index is now stable per (poll, user): all of one voter's option
--     rows in a poll share one index, so the anonymized dump still groups a
--     voter's selections without revealing identity. Its per-poll uniqueness
--     constraint is therefore dropped.
--   * the trigger enforces single-choice for non-multi kinds (a second, distinct
--     option by the same user is skipped) and reuses a voter's existing index
--     on their later option rows in a multi poll.
--
-- Migrations are append-only; never edit this file once applied.

-- Allow the 'multi' kind.
alter table polls drop constraint polls_kind_check;
alter table polls
    add constraint polls_kind_check check (kind in ('single', 'yesno', 'scale', 'multi'));

-- Rework the vote uniqueness for multi-select.
alter table poll_votes drop constraint poll_votes_poll_id_voter_id_key; -- was UNIQUE (poll_id, user_id)
alter table poll_votes drop constraint poll_votes_poll_voter_key;       -- was UNIQUE (poll_id, voter_index)
alter table poll_votes
    add constraint poll_votes_poll_user_option_key unique (poll_id, user_id, option_id);

-- Rewrite the chain trigger. Order matters: lock the head to serialize concurrent
-- votes, resolve the voter's existing index, reject repeats, then chain and
-- advance. next_voter_index only advances for a genuinely new voter, so a
-- multi-voter's several option rows keep one shared index.
create or replace function poll_votes_chain() returns trigger language plpgsql as $$
declare
    h_seq bigint;
    h_hash bytea;
    n_idx bigint;
    p_kind text;
    existing_idx bigint;
begin
    insert into poll_chains(poll_id, head_hash)
        values (new.poll_id, vote_chain_genesis(new.poll_id))
        on conflict (poll_id) do nothing;
    select head_seq, head_hash, next_voter_index
        into h_seq, h_hash, n_idx
        from poll_chains where poll_id = new.poll_id for update;

    select kind into p_kind from polls where id = new.poll_id;

    -- This voter's existing index in this poll, if they have voted before.
    select voter_index into existing_idx
        from poll_votes where poll_id = new.poll_id and user_id = new.user_id
        limit 1;

    -- Reject repeats. For a single-choice poll any prior vote by the user is a
    -- repeat; for multi it is only a repeat of the same option (the unique
    -- constraint is the backstop). A skipped vote must not advance the head.
    if p_kind is distinct from 'multi' then
        if existing_idx is not null then
            return null;
        end if;
    elsif exists (
        select 1 from poll_votes
        where poll_id = new.poll_id and user_id = new.user_id and option_id = new.option_id
    ) then
        return null;
    end if;

    new.seq := h_seq + 1;
    new.voter_index := coalesce(existing_idx, n_idx);
    new.row_hash := vote_chain_hash(
        h_hash, new.poll_id, new.seq, new.option_id, new.voter_index, new.cast_at
    );

    update poll_chains set
        head_seq = new.seq,
        head_hash = new.row_hash,
        -- Only a new voter consumes the next index.
        next_voter_index = case when existing_idx is null then n_idx + 1 else n_idx end
    where poll_id = new.poll_id;
    return new;
end $$;
