-- A staging column for proposed party and person summaries, mirroring the news
-- summary_draft flow. A draft is written here (for example by an offline
-- drafting pass over the structured data), reviewed by an admin, and only on
-- approval promoted to the public summary. Readers never see a draft.
alter table parties add column summary_draft text;
alter table people add column summary_draft text;
