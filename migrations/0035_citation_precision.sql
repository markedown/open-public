-- Make a citation checkable, not just followable.
--
-- A source row says which document a fact came from, but two things it does not
-- say are exactly what a reader needs. Which version of that document we read:
-- `content_hash` carries a key that makes re-importing idempotent, not a hash of
-- anything, so a document that changed after we read it looks identical to one
-- that did not. And where to look if the original is gone: political documents
-- rot faster than most, and a dead link takes the evidence with it.
ALTER TABLE sources
    -- The hash of the exact bytes we read, when we read bytes. Left null where
    -- we cite a document we did not download; an invented hash would be worse
    -- than none, because it would look like proof.
    ADD COLUMN content_sha256 text,
    -- An archived copy of the document, so a citation survives the original.
    ADD COLUMN snapshot_url text;

-- Where in the document the quote is: a page, an article, a decree number, a
-- roll call. This belongs to the citation and not to the source, because two
-- readings can cite different pages of one document, which is exactly what the
-- party programmes do.
ALTER TABLE position_evidence
    ADD COLUMN locator text;
