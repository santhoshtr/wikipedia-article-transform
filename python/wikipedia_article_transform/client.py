from __future__ import annotations

import urllib.error
import urllib.request


USER_AGENT = "wikipedia-article-transform/0.1 (https://github.com/smc/wikisentences)"


def fetch_article_html(language: str, title: str, timeout: float = 20.0) -> str:
    url = f"https://{language}.wikipedia.org/api/rest_v1/page/html/{title}?stash=false"
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:  # nosec B310
            status = getattr(resp, "status", 200)
            if status < 200 or status >= 300:
                raise RuntimeError(f"Failed to fetch article: HTTP {status}")
            return resp.read().decode("utf-8")
    except urllib.error.HTTPError as exc:
        raise RuntimeError(f"Failed to fetch article: HTTP {exc.code}") from exc
    except urllib.error.URLError as exc:
        raise RuntimeError(f"Failed to fetch article: {exc.reason}") from exc
