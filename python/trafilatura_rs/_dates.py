"""Whether the native dateparser port may stand in for htmldate's `_external_date` (see
docs/metadata-formats-scope.md): it reproduces htmldate's own parser only."""

from datetime import time

_PARSER = {
    "languages": None,
    "locales": None,
    "region": None,
    "try_previous_locales": False,
    "use_given_order": False,
    "detect_languages_function": None,
}
_MODIFIED = {
    "NORMALIZE": True,
    "PARSERS": ["custom-formats", "absolute-time"],
    "PREFER_DATES_FROM": "past",
    "PREFER_LOCALE_DATE_ORDER": True,
    "RETURN_AS_TIMEZONE_AWARE": False,
    "STRICT_PARSING": True,
}
_SETTINGS = {
    **_MODIFIED,
    "DATE_ORDER": "MDY",
    "STRICT_DATE_ORDER": "none",
    "TIMEZONE": "local",
    "TO_TIMEZONE": False,
    "TIMEZONE_ABBREVIATIONS": {},
    "PREFER_DAY_OF_MONTH": "current",
    "PREFER_MONTH_OF_YEAR": "current",
    "RELATIVE_BASE": False,
    "REQUIRE_PARTS": [],
    "SKIP_TOKENS": ["t"],
    "DEFAULT_LANGUAGES": [],
    "USE_GIVEN_LANGUAGE_ORDER": False,
    "RETURN_TIME_SPAN": False,
    "RETURN_TIME_AS_PERIOD": False,
    "IGNORE_SURROUNDING_TEXT": False,
}
_PARTS_OF_DAY = {"early_morning": 3, "morning": 9, "afternoon": 15, "evening": 18, "night": 20}


def _local_zone_found():
    try:
        import tzlocal

        tzlocal.get_localzone()
    except Exception:  # dateparser fails to parse without a local zone
        return False
    return True


def stock_external_date(function, parser):
    """Whether `function` is htmldate's `_external_date` and `parser` its `get_external_parser()` as htmldate builds
    it, with a local timezone tzlocal can find."""
    if getattr(function, "__module__", None) != "htmldate.extractors":
        return False
    if getattr(function, "__qualname__", None) != "_external_date":
        return False
    if (type(parser).__module__, type(parser).__qualname__) != ("dateparser.date", "DateDataParser"):
        return False
    if any(getattr(parser, k, object()) != v for k, v in _PARSER.items()):
        return False
    settings = getattr(parser, "_settings", None)
    if getattr(settings, "_mod_settings", None) != _MODIFIED:
        return False
    if any(getattr(settings, k, object()) != v for k, v in _SETTINGS.items()):
        return False
    parts = getattr(settings, "PARTS_OF_DAY", None)
    if any(getattr(parts, k, None) != time(h) for k, h in _PARTS_OF_DAY.items()):
        return False
    return _local_zone_found()
