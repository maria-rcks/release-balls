# add the animation to a release

copy [examples/release.yml](../examples/release.yml) to `.github/workflows/release-balls.yml` in your repository. see the [private action access requirement](#private-action-access) first.

publish a release with github-generated notes or full merged pr links. the example url-encodes the tag for asset links, including tags containing `/` or `#`. the workflow:

1. Renders PR authors in the triggering release. The action defaults to its exact tag and the `author` metric.
2. attaches its gif and mp4 as release assets.
3. adds the linked gif and video download to the existing notes.
4. saves the source data and rankings as an actions artifact.

github scopes caches by ref. a new release tag may need a cold build unless a run on the default branch has already cached the same action binary. reruns on the same tag can reuse its cache.

rerunning the job replaces the same assets and its marked section of the notes. your original notes stay intact. `contents: write` permits publication; `pull-requests: read` permits reading private pr details. the action itself only renders files.

`tag` defaults to `${{ github.event.release.tag_name }}` on release events. It is an exact, case-sensitive selection and overrides `releases` and `match`. For backfills, set it explicitly. To aggregate multiple releases during a release event, set `tag: ''` and choose `releases` and `match`. Outside release events, an unset tag selects the latest published release (one release, no tag filter). `match` is a substring search for groups such as `nightly`; `match: 0.12.1` could also select `0.12.17`.

if another workflow creates your release using its default `GITHUB_TOKEN`, github will not start a second workflow for the resulting release event. put the render/upload steps directly after `gh release create` in that workflow, or publish using an appropriately scoped github app token. a release published through the website or a user token triggers this example normally.

for private repositories, assets require authentication. the download links work for authorized users, but github's image proxy may not display private release-asset gifs inline. for a reliably visible embed, host only the intended gif at a publicly readable url and use that url in your notes. never make the repository public just to embed a gif.

## pass files to another job

upload `${{ steps.balls.outputs.directory }}` with `actions/upload-artifact@v4`, then download that artifact in the next job with `actions/download-artifact@v4`. action outputs are paths on the current runner, so passing a path alone to a different job will not transfer the files. after downloading, open `manifest.json`, `data.json`, and `embed.md` relative to the download directory. the file paths inside the manifest refer to the original render job.

[real workflow runs and results](e2e.md)

## Private action access

This action is shared with other private `maria-rcks` repositories. For another account, copy it into your own private repository, enable **Settings → Actions → General → Access**, and change the workflow's `uses:` reference. Passing a token to the action does not grant access to the action itself.

For private source PRs, add `pull-requests: read` to the workflow permissions. To read a different private source repository, set `token: ${{ secrets.SOURCE_READ_TOKEN }}` to a token with access to that repository's releases and pull requests.
