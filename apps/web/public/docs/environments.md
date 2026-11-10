# Projects and environments

> Projects, environments, inheritance and references between variables.

A vault holds projects, and each project holds environments such as development, staging and production. Every command works on the environment linked to the current directory, usually development. Pass `-e <env>` to use another.

## Projects

A project is created the first time you write to it from a directory, along with a `.dotsafe.json` link file. Commit that file. To attach a directory to an existing project, or change its default environment:

```bash
dotsafe link my-api                    # this directory now uses the my-api project
dotsafe link my-api -e staging         # and staging by default
```

## Inheritance

An environment can inherit another one in the same project. It gets all of the parent's variables, and its own values win.

```bash
dotsafe inherit production -e staging       # staging starts from production
dotsafe set -e staging LOG_LEVEL=debug      # and overrides one value
dotsafe unset -e staging --hide SENTRY_DSN  # drops one it shouldn't have
dotsafe inherit --none -e staging           # stop inheriting
```

`unset` on its own removes the environment's own value, so the inherited one applies again. `--hide` removes the variable entirely. In the app, inherited and overridden variables are marked in the list.

## References

A value can include other variables from the same environment with `${{ NAME }}`. They're filled in when the value is read, so changing `DB_HOST` updates `DB_URL` too.

```bash
dotsafe set DB_HOST=db.internal
dotsafe set 'DB_URL=postgres://${{ DB_HOST }}/app'   # single quotes, so the shell leaves it alone
```

> [!NOTE]
> A reference to a variable that doesn't exist is an error, never an empty string, so a typo can't quietly reach your app.

## Comparing environments

`dotsafe diff` shows what's only in one environment and which values differ. It prints names, never values, and exits 1 when anything differs.

```bash
dotsafe diff production                  # the linked environment against production
dotsafe diff staging production --names  # names only; values are expected to differ
```

In the app, press `⌘D` or choose Compare in the project's ⋯ menu. A variable missing from one side can be added there; you type its value.

## .env.example

Keep a committed `.env.example` in step with the vault, and check that an environment has everything it lists.

```bash
dotsafe example -o .env.example   # names only; values the file already had are kept
dotsafe check                     # fails if a variable in .env.example isn't set
dotsafe check -e production --strict   # also fails on variables the file doesn't list
```

`dotsafe check` exits 1 when something is missing, so it fits a pre-commit hook or a deploy script.

## In the app

- `⌘K` finds a variable in any project and copies it, or jumps to an environment.
- `⌥⌘K` or the menu bar icon does the same from any other app.
- Copied values are cleared from the clipboard after 30 seconds.

To share one value between projects, make it a [secret](/docs/secrets).
