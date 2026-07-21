-- Upcoming elections, and a compass that can be about people rather than parties.
--
-- 1. An election row already carries a date, so an election dated in the future
--    is simply one that has not happened yet. What it also needs is a way to say
--    how firm that date is: some systems fix the date by law, others only fix a
--    deadline ("by June 2028 unless parliament dissolves early"). Without that
--    the page would present a legally provisional date as a certainty.
ALTER TABLE elections
    ADD COLUMN expected_note text;

-- 2. A parliamentary election is contested by parties, a presidential one by
--    people. The compass therefore needs to score whichever kind of contestant
--    the election actually has, so a thesis set declares its scope and a piece
--    of evidence attaches to exactly one of a party or a person, the same
--    "exactly one of the two" rule statements already follow.
ALTER TABLE theses
    ADD COLUMN scope text NOT NULL DEFAULT 'party'
    CHECK (scope IN ('party', 'person'));

ALTER TABLE position_evidence
    ADD COLUMN person_id bigint REFERENCES people (id);

ALTER TABLE position_evidence
    ALTER COLUMN party_id DROP NOT NULL;

ALTER TABLE position_evidence
    ADD CONSTRAINT position_evidence_contestant_check
    CHECK ((party_id IS NOT NULL) <> (person_id IS NOT NULL));

-- The old uniqueness was (thesis, party, kind, source). Widen it to cover a
-- person contestant too. NULLS NOT DISTINCT so the unused column still collides
-- rather than letting the same reading be inserted twice.
ALTER TABLE position_evidence
    DROP CONSTRAINT position_evidence_thesis_id_party_id_kind_source_id_key;

ALTER TABLE position_evidence
    ADD CONSTRAINT position_evidence_contestant_source_key
    UNIQUE NULLS NOT DISTINCT (thesis_id, party_id, person_id, kind, source_id);

CREATE INDEX position_evidence_person_idx ON position_evidence (person_id);
