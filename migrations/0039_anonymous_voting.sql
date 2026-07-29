-- Anonymous voting: per-poll blind-signature issuer keys and the anonymous
-- ballot store. See docs/anonymous-voting.md.
--
-- A verified account gets a blind-signed token per poll (the server signs a
-- value it never sees); voting spends the token anonymously. The server sees
-- that an account participated, never how it voted, and cannot compute the link.
--
-- Migrations are append-only; never edit this file once applied.

-- One RFC 9474 RSA blind-signature issuer keypair per poll
-- (RSABSSA-SHA384-PSS-Randomized). The private key is destroyed when the poll
-- closes, so a later compromise cannot forge into a closed poll; the public key
-- stays for verification. Keys are DER-encoded.
create table poll_issuer_keys (
    poll_id     bigint primary key references polls (id) on delete cascade,
    public_key  bytea not null,
    private_key bytea,                       -- null once destroyed at close
    created_at  timestamptz not null default now()
);

-- "Account A was issued a token for poll P." Carries no token and no signature:
-- the server blind-signed a value it never saw, so this row cannot be linked to
-- any ballot. One entitlement per account per poll.
create table vote_entitlements (
    id         bigserial primary key,
    poll_id    bigint not null references polls (id) on delete cascade,
    user_id    bigint not null references users (id),
    issued_at  timestamptz not null default now(),
    unique (poll_id, user_id)
);

-- The anonymous votes. `token` is the nullifier: unique per poll, tied to no
-- account. `signature` and `msg_randomizer` let anyone verify the token was
-- issued for this poll. Hash-chained per poll for tamper-evidence, like
-- poll_votes.
create table vote_ballots (
    id             bigserial primary key,
    poll_id        bigint not null references polls (id) on delete cascade,
    token          bytea not null,
    signature      bytea not null,
    msg_randomizer bytea,                    -- RFC 9474 randomizer, published for verify
    seq            bigint not null,
    content_hash   bytea not null,
    prev_hash      bytea,
    cast_at        timestamptz not null default now(),
    unique (poll_id, token),
    unique (poll_id, seq)
);

-- The options a ballot selected (one row for single-choice, several for multi).
create table ballot_options (
    ballot_id  bigint not null references vote_ballots (id) on delete cascade,
    option_id  bigint not null references poll_options (id),
    primary key (ballot_id, option_id)
);
