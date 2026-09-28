---
name: contract
description: The message format for the neoplanner:agent agent, the only writer of this project's OpenSpec specs and changes. Load it before sending the agent any request (to plan a change, revise one, report progress on its tasks, archive it, or ask about the specs) and when a hook refuses a request to the agent or a write to the OpenSpec artifacts.
---

# Talking to neoplanner:agent

The `neoplanner:agent` agent writes this project's OpenSpec specs and changes. You read
them, at the absolute paths it gives you, and write the code. Send it one JSON object as
its whole task, and nothing else. A hook checks every request against this contract, and
refuses one that doesn't fit, saying exactly what to fix. A change is named in kebab-case,
such as `add-magic-link-login`.

## Plan a change

Before substantial work that no change covers yet, such as a feature, a behaviour change or
a refactor that crosses modules. Small fixes need no change. Run it in the
**background**, and carry on with anything that doesn't depend on the plan:

```json
{"kind": "plan", "request": "Let users sign in with a magic link sent by email",
 "why": "Passwords are the top support issue", "constraints": ["No new runtime dependencies"]}
```

`request` is required. `why`, `constraints`, and `change` (to continue an existing change,
or to choose the new one's name) are optional. When the reply comes, read the file at
`artifacts.tasks`, and `artifacts.design` if you need it, then implement.

## Report progress

As you finish tasks, depart from the design, or get blocked. Run it in the background.
Every list is optional, but at least one must have something in it:

```json
{"kind": "progress", "change": "add-magic-link-login",
 "done": ["1.1", "1.2"],
 "deviations": [{"what": "Tokens are stored hashed", "why": "Review asked for it"}],
 "blocked": [{"task": "3.2", "why": "No staging mail server"}]}
```

## Revise the plan

When implementation shows the plan wrong. Run it in the foreground, then reread what it
changed:

```json
{"kind": "revise", "change": "add-magic-link-login", "feedback": "The mailer can't run in a background job; send tokens inline"}
```

## Ask, and archive

```json
{"kind": "ask", "question": "What does the spec say about session expiry?"}
```

```json
{"kind": "archive", "change": "add-magic-link-login"}
```

Run an ask in the foreground. Archive only when the user asks for it, or agrees once every
task is done.

## Replies

The agent answers with one JSON object. Its `status` follows the A2A task states:

- **`completed`**: `summary` or `answer` is for you, and for the user when it's news to
  them. `artifacts` maps each artifact to its absolute path, `written` lists the files it
  changed, and `next` is your next step.
- **`input-required`**: ask the user its `questions` word for word, with their `options`,
  and send the answers to **the same agent**, with SendMessage, in the same order:

  ```json
  {"kind": "answers", "answers": ["Yes, only the newest link works"]}
  ```

- **`failed`**: tell the user its `reason`.
