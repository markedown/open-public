-- Generalize an election result's contestant: a party, or a plain label.
--
-- Party elections attribute votes to a party (party_id). Other contests do not:
-- a presidential election is per-candidate and a referendum is per-option
-- (Evet/Hayır). For those, the contestant is a free label instead of a party.
-- Exactly one of party_id / label identifies the contestant.
--
-- Migrations are append-only; never edit this file once applied.

alter table election_results
    alter column party_id drop not null,
    add column label text;

alter table election_results
    add constraint election_results_contestant_ck
        check ((party_id is not null) <> (label is not null));

-- Label contestants are unique per election (nulls are distinct, so party rows
-- with a null label never collide here).
create unique index election_results_election_label_key
    on election_results (election_id, label);
