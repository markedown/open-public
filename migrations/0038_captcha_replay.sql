-- Replay guard for the proof-of-work captcha (ALTCHA).
--
-- A solved challenge is single-use: its HMAC signature is recorded here on the
-- first successful verification, and a second submission of the same solution
-- is rejected. Rows are short-lived (a challenge expires within minutes) and
-- pruned as they are checked, so this stays tiny.
--
-- No personal data: the signature is an HMAC of a random challenge, tied to no
-- account and no address.
--
-- Migrations are append-only; never edit this file once applied.
create table captcha_used (
    signature  text primary key,
    expires_at timestamptz not null
);

create index captcha_used_expires_at on captcha_used (expires_at);
