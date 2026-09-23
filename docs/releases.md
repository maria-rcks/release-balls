# Add the animation to a release

Copy [examples/release.yml](../examples/release.yml) to `.github/workflows/release-balls.yml` in your repository.

Publish a release with GitHub-generated notes or full merged PR links. The example URL-encodes the tag for asset links, including tags containing `/` or `#`. The workflow:

1. Renders all matching PR authors in the triggering release. The action defaults to its exact tag and the `author` metric.
2. Attaches its GIF and MP4 as release assets.
3. Adds the linked GIF and video download to the existing notes.
4. Saves the source data and rankings as an Actions artifact.

Every matching contributor stays visible in every frame. Names, counts, and balls scale uniformly into additional columns as the list grows. The default clip remains 16 seconds at any contributor count. Very large lists have tiny labels, especially in the 480px GIF; use a higher-resolution MP4 when individual names matter. The CLI only limits contributors when you pass `--top N`; bots are excluded unless you pass `--include-bots`.

GitHub scopes caches by ref. A new release tag may need a cold build unless a run on the default branch has already cached the same action binary. Reruns on the same tag can reuse its cache.

Rerunning the job replaces the same assets and its marked section of the notes. Your original notes stay intact. `contents: write` permits publication; `pull-requests: read` permits reading private PR details. The action itself only renders files.

## Selecting releases

`tag` defaults to `${{ github.event.release.tag_name }}` on release events. It is an exact, case-sensitive selection and overrides `releases` and `match`. For backfills, set it explicitly. To aggregate multiple releases during a release event, set `tag: ''` and choose `releases` and `match`. Outside release events, an unset tag selects the latest published release (one release, no tag filter). `match` is a substring search for groups such as `nightly`; `match: 0.12.1` could also select `0.12.17`.

## Releases created by other workflows

If another workflow creates your release using its default `GITHUB_TOKEN`, GitHub will not start a second workflow for the resulting release event. Put the render and upload steps directly after `gh release create` in that workflow, or publish using an appropriately scoped GitHub App token. A release published through the website or a user token triggers this example normally.

## Private repositories

Assets in private repositories require authentication. The download links work for authorized users, but GitHub's image proxy may not display private release-asset GIFs inline. For a reliably visible embed, host only the intended GIF at a publicly readable URL and use that URL in your notes. Never make the repository public just to embed a GIF.

For private source PRs, add `pull-requests: read` to the workflow permissions. To read a different private source repository, set `token: ${{ secrets.SOURCE_READ_TOKEN }}` to a token with access to that repository's releases and pull requests.

## Pass files to another job

Upload `${{ steps.balls.outputs.directory }}` with `actions/upload-artifact@v4`, then download that artifact in the next job with `actions/download-artifact@v4`. Action outputs are paths on the current runner, so passing a path alone to a different job will not transfer the files. After downloading, open `manifest.json`, `data.json`, and `embed.md` relative to the download directory. The file paths inside the manifest refer to the original render job.
