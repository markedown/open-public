-- Widen the evidence kinds so an opposition party can be judged on its record
-- too, not only on what it promised.
--
-- Governing parties act by passing laws and issuing decrees, which the existing
-- kinds already cover. Opposition parties cannot legislate: they act by tabling
-- bills and by asking the Constitutional Court to annul a law. Without these
-- kinds, the recorded-action tier is available only to whoever is in power, so
-- the compass would systematically judge the government on its deeds and every
-- other party on its promises. That is not neutrality, it is a bias built into
-- the sourcing.
--
-- Both new kinds are recorded action: a party filed a document with a state
-- institution on a date, which is a fact about what it did.
ALTER TABLE position_evidence DROP CONSTRAINT position_evidence_kind_check;

ALTER TABLE position_evidence
    ADD CONSTRAINT position_evidence_kind_check
    CHECK (kind IN ('manifesto', 'statement', 'bill', 'court', 'vote', 'law', 'decree', 'alliance'));
