-- Most recent end marker per friendship. Messages with larger IDs start a new visit.
CREATE TABLE conversation_ends (
    member_low INTEGER NOT NULL,
    member_high INTEGER NOT NULL,
    ended_by INTEGER NOT NULL,
    through_message_id INTEGER NOT NULL CHECK (through_message_id >= 0),
    PRIMARY KEY (member_low, member_high),
    FOREIGN KEY (member_low, member_high) REFERENCES relationships(member_low, member_high),
    CHECK (ended_by IN (member_low, member_high))
);

CREATE INDEX conversation_ends_by_high ON conversation_ends(member_high);
