-- User accounts, sessions, and admin support.
--
-- Replaces the anonymous-voter model with registered users who have an
-- email+password account and an optional admin flag. Cast votes remain
-- immutable (the trigger on poll_votes is unchanged).
--
-- Includes the NULLS NOT DISTINCT fix on time-range unique keys so that
-- idempotent ingest does not duplicate rows with NULL start dates.
--
-- Migrations are append-only; never edit this file once applied.

-- 1. Transform voters into users ------------------------------------------------

alter table voters rename to users;

alter table users
    add column password_hash text,
    add column is_admin      boolean not null default false,
    add column created_at    timestamptz not null default now();

-- Registration creates a user before verification, so verified_at becomes
-- nullable (it is set when the email is confirmed).
alter table users
    alter column verified_at drop not null,
    alter column verified_at drop default;

-- 2. poll_votes: voter_id → user_id ----------------------------------------------

alter table poll_votes
    drop constraint poll_votes_voter_id_fkey;

alter table poll_votes
    rename column voter_id to user_id;

alter table poll_votes
    add constraint poll_votes_user_id_fkey
        foreign key (user_id) references users(id);

-- 3. email_verifications: link back to the user ----------------------------------
-- The old design matched on email_hash alone; now we also store the user row
-- so the verification handler can mark the correct user as verified.

alter table email_verifications
    add column user_id bigint references users(id);

-- 4. Sessions (login state) ------------------------------------------------------

create table sessions (
    id         bigserial primary key,
    user_id    bigint not null references users(id) on delete cascade,
    token_hash text not null unique,
    expires_at timestamptz not null,
    created_at timestamptz not null default now()
);

create index sessions_token_hash_idx on sessions(token_hash);
create index sessions_user_id_idx   on sessions(user_id);

-- 5. NULLS NOT DISTINCT: Postgres treats NULLs as distinct in unique
--    constraints by default, so a re-run of idempotent ingest would insert
--    duplicate memberships or roles whose key contains a NULL start date.
--    NULLS NOT DISTINCT (Postgres 15+) treats those NULLs as equal.

alter table party_memberships
    drop constraint party_memberships_person_id_party_id_start_date_key,
    add constraint party_memberships_person_id_party_id_start_date_key
        unique nulls not distinct (person_id, party_id, start_date);

alter table roles
    drop constraint roles_person_id_role_type_org_start_date_key,
    add constraint roles_person_id_role_type_org_start_date_key
        unique nulls not distinct (person_id, role_type, org, start_date);
