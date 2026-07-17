-- User-submitted polls: a moderation pipeline plus uploaded image assets.
--
-- Until now polls were admin-only and the platform held no user-authored free
-- text. This opens poll creation to verified users while keeping the safety
-- guarantees: a submission is screened by an automated reviewer and then by a
-- human admin, and nothing a user wrote or uploaded is public until it has
-- passed both. On approval a real `polls` row is created from the submission.
--
-- These rows are proposals and media, not sourced facts, so (like `polls`
-- itself) they carry no `source_id`; provenance here is the submitting user.
--
-- Migrations are append-only; never edit this file once it has been applied.

-- An uploaded image. The bytes are validated, re-encoded to a normalized raster
-- format (stripping all metadata), and written to disk addressed by their hash;
-- this table is the metadata and the moderation-visibility record. `mime`,
-- `width` and `height` describe the re-encoded output we control, never the
-- original upload.
create table assets (
    id bigserial primary key,
    -- Hex sha256 of the re-encoded bytes; also the on-disk name and the public
    -- URL path segment. Unique, so identical images are stored once.
    sha256 text not null unique,
    mime text not null check (mime in ('image/png', 'image/jpeg')),
    width int not null,
    height int not null,
    byte_size bigint not null,
    uploaded_by bigint not null references users (id),
    -- True once referenced by an approved, published poll: only then is the
    -- image servable to anyone. While false it is visible to its uploader and
    -- to admins alone, so pending (unreviewed) uploads never leak.
    published boolean not null default false,
    created_at timestamptz not null default now()
);

create index assets_uploaded_by_idx on assets (uploaded_by);

-- A permanent suspension after repeated policy violations. A banned account's
-- sessions stop resolving and it cannot sign in again. Cast votes are never
-- touched by a ban; they remain append-only.
alter table users
    add column banned_at timestamptz,
    add column ban_reason text;

-- A proposed poll awaiting moderation. It mirrors a poll's shape but is not one
-- until approved, at which point `published_poll_id` records the poll created.
create table poll_submissions (
    id bigserial primary key,
    submitter_id bigint not null references users (id),
    country_id bigint not null references countries (id),
    question text not null,
    kind text not null default 'single'
        check (kind in ('single', 'yesno', 'scale', 'multi')),
    question_asset_id bigint references assets (id),
    status text not null default 'pending_ai'
        check (status in ('pending_ai', 'ai_rejected', 'pending_admin', 'approved', 'rejected')),
    -- The automated pre-screen result.
    ai_model text,
    ai_decision text check (ai_decision in ('allow', 'reject')),
    ai_reason text,
    ai_categories text[],
    ai_reviewed_at timestamptz,
    -- How many times the automated screen has been attempted. After repeated
    -- failures (the reviewer is unreachable) the submission falls back to the
    -- admin queue rather than staying stuck.
    ai_attempts int not null default 0,
    -- Set when this submission is a policy violation: an automated hard-reject,
    -- or an admin marking it so. Violations accrue toward the ban threshold.
    is_violation boolean not null default false,
    -- The admin decision.
    reviewed_by bigint references users (id),
    admin_note text,
    reviewed_at timestamptz,
    published_poll_id bigint references polls (id),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create index poll_submissions_status_idx on poll_submissions (status);
create index poll_submissions_submitter_idx on poll_submissions (submitter_id);

-- One answer option of a submitted poll: a label and an optional uploaded image.
create table poll_submission_options (
    id bigserial primary key,
    submission_id bigint not null references poll_submissions (id) on delete cascade,
    label text not null,
    position int not null,
    asset_id bigint references assets (id)
);

create index poll_submission_options_submission_idx
    on poll_submission_options (submission_id);
