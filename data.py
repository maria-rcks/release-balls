"""Collect merged PRs explicitly linked by a repository's published release notes.

This is changelog membership, not a merge-date window or a full commit ancestry
analysis. Bare #123 references and external repository links are not counted.
GitHub-generated notes with no change entries are accepted as empty; unsupported
notes fail loudly rather than silently reporting zero activity.
"""

import concurrent.futures
import datetime
import json
import os
import re
import subprocess
import time
import urllib.error
import urllib.request


def _token():
    token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    if not token:
        try:
            result = subprocess.run(
                ["gh", "auth", "token"], capture_output=True, text=True, timeout=10
            )
            if result.returncode == 0:
                token = result.stdout.strip()
        except (OSError, subprocess.TimeoutExpired):
            pass
    return token


def _get(path, token, payload=None):
    headers = {
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "release-balls",
    }
    if token:
        headers["Authorization"] = "Bearer " + token
    if payload is not None:
        headers["Content-Type"] = "application/json"
        payload = json.dumps(payload).encode()
    for attempt in range(4):
        try:
            request = urllib.request.Request("https://api.github.com" + path, data=payload, headers=headers)
            with urllib.request.urlopen(request, timeout=30) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            limited = error.code == 403 and (
                error.headers.get("X-RateLimit-Remaining") == "0"
                or error.headers.get("Retry-After")
            )
            if attempt == 3 or not (limited or error.code == 429 or error.code >= 500):
                raise RuntimeError(f"GitHub API returned HTTP {error.code} for {path}") from None
            delay = float(error.headers.get("Retry-After", 2 ** attempt))
            if limited and error.headers.get("X-RateLimit-Reset"):
                delay = max(delay, float(error.headers["X-RateLimit-Reset"]) - time.time() + 1)
            if delay > 60:
                raise RuntimeError("GitHub rate limit exceeded; retry later with GH_TOKEN") from None
            time.sleep(max(0, delay))
        except (urllib.error.URLError, TimeoutError):
            if attempt == 3:
                raise RuntimeError(f"Could not reach GitHub API for {path}") from None
            time.sleep(2 ** attempt)


def _graphql_pulls(repo, numbers, token):
    owner, name = repo.split("/")
    pulls = {}
    for offset in range(0, len(numbers), 50):
        batch = numbers[offset:offset + 50]
        fields = " ".join(
            f"p{number}: pullRequest(number:{number}) "
            "{ number title url author { login } mergedBy { login } mergedAt additions deletions }"
            for number in batch
        )
        query = (
            "query($owner:String!,$name:String!) { repository(owner:$owner,name:$name) { "
            + fields + " } }"
        )
        result = _get("/graphql", token, {"query": query, "variables": {"owner": owner, "name": name}})
        if result.get("errors"):
            raise RuntimeError("GitHub GraphQL query unavailable")
        items = result["data"]["repository"]
        for number in batch:
            item = items[f"p{number}"]
            if not item["mergedAt"]:
                raise ValueError(f"Release notes link unmerged PR {repo}#{number}")
            pulls[number] = {
                "number": item["number"], "title": item["title"], "url": item["url"],
                "author": (item.get("author") or {}).get("login"),
                "merger": (item.get("mergedBy") or {}).get("login"),
                "merged_at": item["mergedAt"],
                "additions": item["additions"], "deletions": item["deletions"],
            }
    return pulls


def _numbers(repo, release):
    body = release.get("body") or ""
    pattern = r"https://github\.com/" + re.escape(repo) + r"/pull/(\d+)(?!\d)\b"
    numbers = sorted({int(number) for number in re.findall(pattern, body, re.IGNORECASE)})
    if numbers:
        return numbers
    empty = re.search(r"\bno (?:changes|pull requests|commits)\b", body, re.IGNORECASE)
    generated = re.search(r"^## What's Changed\s*$", body, re.MULTILINE)
    entries = re.search(r"^\s*[-*]\s+", body, re.MULTILINE)
    if empty or (generated and not entries):
        return []
    raise ValueError(
        f"{release['tag_name']}: no recognized {repo} PR links in release notes; "
        "expected full GitHub pull request URLs or an explicitly empty changelog"
    )


def collect(repo, limit=5, match="nightly"):
    """Return newest published matching releases and their linked, merged PRs.

    All release pages are examined because GitHub orders by creation rather than
    publication. Authenticated PR reads use GraphQL batches of at most 50, falling
    back to REST with at most six requests active if GraphQL is unavailable.
    """
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repo):
        raise ValueError("repo must be owner/repository")
    if not isinstance(limit, int) or limit < 1:
        raise ValueError("limit must be a positive integer")
    token = _token()
    releases = []
    page = 1
    while True:
        batch = _get(f"/repos/{repo}/releases?per_page=100&page={page}", token)
        releases.extend(
            item for item in batch
            if not item["draft"] and item.get("published_at")
            and match.lower() in item["tag_name"].lower()
        )
        if len(batch) < 100:
            break
        page += 1
    releases.sort(key=lambda item: item["published_at"], reverse=True)
    releases = releases[:limit]
    if len(releases) < limit:
        raise ValueError(f"Found only {len(releases)} published releases matching {match!r}; need {limit}")
    membership = [_numbers(repo, release) for release in releases]
    numbers = sorted({number for items in membership for number in items})

    def pull(number):
        item = _get(f"/repos/{repo}/pulls/{number}", token)
        if not item.get("merged_at"):
            raise ValueError(f"Release notes link unmerged PR {repo}#{number}")
        return number, {
            "number": number,
            "title": item["title"],
            "url": item["html_url"],
            "author": (item.get("user") or {}).get("login"),
            "merger": (item.get("merged_by") or {}).get("login"),
            "merged_at": item["merged_at"],
            "additions": item["additions"],
            "deletions": item["deletions"],
        }

    pulls = None
    if token:
        try:
            pulls = _graphql_pulls(repo, numbers, token)
        except (RuntimeError, KeyError, TypeError):
            pass  # GraphQL permissions can differ from REST permissions.
    if pulls is None:
        with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
            pulls = dict(pool.map(pull, numbers))
    return {
        "repo": repo,
        "fetched_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "scope": "merged PRs explicitly linked in each release's notes; deduplicated per release",
        "releases": [
            {
                "tag": release["tag_name"],
                "published_at": release["published_at"],
                "url": release["html_url"],
                "prs": [pulls[number] for number in items],
            }
            for release, items in zip(releases, membership)
        ],
    }
