# Manage Python environment variables without .env files

> Keep Django, Flask and FastAPI secrets in an encrypted dotsafe vault and inject them into os.environ, instead of python-dotenv.

Python apps usually read secrets with `os.environ`, often loaded from a `.env` file by `python-dotenv`. dotsafe keeps the variables encrypted on your Mac and sets them when your process starts, so `os.environ["DATABASE_URL"]` keeps working.

## Move your .env into dotsafe

```bash
dotsafe import                                 # ./.env into the development environment
rm .env
```

`load_dotenv()` doesn't override variables that are already set, so you can leave it in place while you switch.

## Run your app

```bash
dotsafe run -- python manage.py runserver      # Django
dotsafe run -- flask run                       # Flask
dotsafe run -- uvicorn app.main:app --reload   # FastAPI
dotsafe run -- pytest                          # tests get the same variables
```

With uv or Poetry, put `dotsafe run --` in front: `dotsafe run -- uv run pytest`.

## Production settings

```bash
dotsafe set -e production DJANGO_SECRET_KEY    # asks for the value
dotsafe run -e production -- python manage.py migrate
```

Generate random secrets without ever seeing them:

```bash
dotsafe secret rotate DJANGO_SECRET_KEY --generate
```

## Notebooks and scripts

`dotsafe run -- jupyter lab` gives every notebook kernel your variables, without a key pasted into a cell.

Start with the [quick start](/docs/quick-start).
