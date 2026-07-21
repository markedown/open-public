-- Getting back into an account.
--
-- An account could be created and verified, and a forgotten password had no way
-- back: no route, no token, no mail. The only recovery was someone with database
-- access editing a row, which is not a recovery path, it is an outage for that
-- person.
--
-- The token is stored only as a hash, the same way a session is. A leaked
-- database therefore does not hand over working reset links, and the plaintext
-- token exists only in the mail that was sent.
CREATE TABLE password_resets (
    id bigserial PRIMARY KEY,
    user_id bigint NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash text NOT NULL UNIQUE,
    expires_at timestamptz NOT NULL,
    -- Set the moment the token is used, so a link works exactly once even if
    -- the mail is forwarded or the page is reloaded.
    consumed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX password_resets_user_idx ON password_resets (user_id);
