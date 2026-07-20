-- Evidence behind a party's stance on a thesis.
--
-- A stance used to be a single asserted value with one source, which could only
-- ever express what a party said about itself. That rewards whoever writes the
-- best manifesto: a party can pledge one thing and legislate the opposite, and
-- a single-source stance has to silently pick one.
--
-- A stance is now derived from dated, typed, sourced evidence. Recorded action
-- (a vote, a law, a decree, a governing alliance) outranks stated intention (a
-- manifesto pledge, an official statement), because what a party did is better
-- evidence of where it stands than what it promised. Where the two disagree,
-- both are kept and the disagreement is shown rather than averaged away, the
-- same principle the data_conflicts table already applies to source conflicts.
create table position_evidence (
    id bigserial primary key,
    thesis_id bigint NOT NULL REFERENCES theses (id) ON DELETE CASCADE,
    party_id bigint NOT NULL REFERENCES parties (id),
    -- Ordered loosely from stated intention to recorded action; the query that
    -- resolves a stance treats vote/law/decree/alliance as the stronger tier.
    kind text NOT NULL CHECK (kind IN ('manifesto', 'statement', 'vote', 'law', 'decree', 'alliance')),
    stance smallint NOT NULL CHECK (stance BETWEEN -2 AND 2),
    -- The wording or reference the reading rests on (a page, an article, a
    -- decree number), so a reader can find it in the cited document.
    quote text,
    -- When the act happened or the document was published. Drives recency
    -- within a tier, so a later law supersedes an earlier one.
    occurred_on date,
    source_id bigint NOT NULL REFERENCES sources (id),
    created_at timestamptz NOT NULL DEFAULT now(),
    -- One reading per document per kind, so re-importing is idempotent.
    UNIQUE (thesis_id, party_id, kind, source_id)
);

CREATE INDEX position_evidence_thesis_party_idx ON position_evidence (thesis_id, party_id);

-- Every stance recorded so far was read from a party manifesto, so it carries
-- over as manifesto evidence with its justification as the quote.
INSERT INTO position_evidence (thesis_id, party_id, kind, stance, quote, source_id)
SELECT thesis_id,
       party_id,
       'manifesto',
       stance,
       justification,
       source_id
  FROM party_positions;

DROP TABLE party_positions;
