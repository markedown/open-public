-- The correction process: a visitor says what on a page reads wrong, an editor
-- checks it against that page's source and closes the report. A report is not a
-- fact row and carries no source of its own; the fix it asks for is made through
-- the normal editing flow, where the correction's source is recorded.
--
-- A contact address is personal data, kept only so an editor can ask a question
-- about one report. Closing a report therefore deletes it: the write path nulls
-- it in the same statement that records the decision, and the schema bounds what
-- may be held while the report is open.
create table corrections (
    id bigserial primary key,
    created_at timestamptz not null default now(),
    -- The site path the report is about, so a queue entry names its own page.
    page text not null check (page like '/%' and char_length(page) <= 300),
    description text not null check (char_length(description) between 10 and 2000),
    source_url text check (
        source_url is null
        or (char_length(source_url) <= 500
            and (source_url like 'http://%' or source_url like 'https://%'))
    ),
    contact_email text check (contact_email is null or char_length(contact_email) <= 254),
    status text not null default 'open' check (status in ('open', 'resolved', 'rejected')),
    resolution_note text check (resolution_note is null or char_length(resolution_note) <= 1000),
    resolved_at timestamptz
);

-- The queue reads open reports oldest first; closed ones stay for the record.
create index corrections_status_created_idx on corrections (status, created_at);
