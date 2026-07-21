-- Records the author of the tag a tag_set displaced via overwrite.
-- Lets removal authorization tell "a tag I added" apart from "a tag I took
-- over from someone else", so the 30 minute self-removal window cannot be
-- used to launder a removal of another user's tag through an overwrite.
ALTER TABLE player_events ADD COLUMN displaced_author BIGINT;
