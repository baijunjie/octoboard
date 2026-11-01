/** An example of the session ID a conversation to resume is asked for by, in the form the agents
 * show it. */
export const EXAMPLE_CONVERSATION_ID = "123e4567-e89b-12d3-a456-426614174000";

/** A conversation ID as the agents write it. The daemon also takes other spellings of a UUID and
 * keeps the lowercase hyphenated one; the field asks for the form the agents show. */
export const CONVERSATION_ID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
