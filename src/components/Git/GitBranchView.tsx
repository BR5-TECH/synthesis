import { ChangedFilesView } from "./ChangedFilesView";
import { RegionNote } from "./parts";
import type { BranchCompareController, BranchRef } from "./useBranchCompare";

/**
 * GIT-FR-GDMG, GIT-FR-SION: the Branches section's large view. It shows what
 * the selected branch changed against its base, side by side with the diff of
 * the selected file (GIT-FR-WMVK). The head names the branch, its base, and
 * the merge base in short form.
 */
export function BranchView({
  branch,
  compare,
}: {
  branch: BranchRef | null;
  compare: BranchCompareController;
}) {
  if (branch === null) {
    return (
      <>
        <div className="git__right-head">
          <span className="t-eyebrow">BRANCH</span>
        </div>
        <RegionNote kind="empty" testId="git-compare-no-branch">
          Select a branch to see what it changed against its base.
        </RegionNote>
      </>
    );
  }
  const data = compare.comparison.data;
  return (
    <>
      <div className="git__right-head" data-testid="git-compare-head">
        <span className="t-eyebrow">BRANCH</span>
        <span className="git__file-name" title={branch.name}>
          {branch.name}
        </span>
        {data && (
          <span className="t-meta" data-testid="git-compare-base">
            vs <span className="t-hash">{data.base}</span>
            {data.mergeBase && (
              <>
                {" "}
                at <span className="t-hash" title={data.mergeBase}>
                  {data.mergeBase.slice(0, 7)}
                </span>
              </>
            )}
          </span>
        )}
      </div>
      {data?.sameAsBase ? (
        <RegionNote kind="empty" testId="git-compare-same-as-base">
          {branch.name} is its own base, so there is nothing to compare.
        </RegionNote>
      ) : (
        <ChangedFilesView
          files={compare.files}
          filePath={compare.filePath}
          onSelectFile={compare.selectFile}
          onRetryFiles={compare.retryFiles}
          diff={compare.diff}
          onRetryDiff={compare.retryDiff}
          diffOwner={branch.name}
          emptyFilesText={
            data
              ? `${branch.name} changed no files against ${data.base}.`
              : "This branch changed no files."
          }
          testIdPrefix="git-compare"
          errorSubject={{ branch: branch.name }}
        />
      )}
    </>
  );
}
