## Issue

When an author starts a new Graduation run on an occupied existing stream and chooses to commit standing work, the optional commit-message field currently defaults to the new draft's name. It must instead use the captured draft name of the newest non-terminal run in the selected stream's queue.

## User story

- **WHEN** the author starts a new Graduation run on an occupied existing stream and chooses a standing-work option that commits.
- **THEN** the optional commit-message field is prefilled with the captured draft name of the newest non-terminal run in the selected stream's queue.
- **WHEN** the author selects a different existing stream while the field is shown.
- **THEN** replace the field value with that stream's default, even if the author edited the previous value.
- This change applies to new Graduation starts only. Restart confirmations keep their current behavior.

## Requirements

- Select the newest run by the project's durable run order, limited to non-terminal runs assigned to the selected stream. Use its captured `input.draft_name`; do not use the new draft's name or a terminal run's name.
- Use the existing `list_graduation_queue` data. Do not add a backend command, field, or change to a command signature.
- Only prefill the optional message field when the existing dialog rules show it. If the author leaves it empty, keep the existing behavior that the run uses its own captured draft name.
- Changing the selected existing stream replaces the field value with that stream's default, including when the author edited the prior value. Keep all other dialog behavior and the user's edits unchanged until the selected stream changes.
- Do not apply this default to restart confirmations or change when the message field is shown, the standing-work choices, commit behavior, or backend behavior.
- Add or update frontend tests for selecting the newest non-terminal run, excluding terminal runs, and replacing an edited message when the selected existing stream changes. Verify that restart confirmation behavior remains unchanged.
- Update `specifications/ui/GSD-graduation-start-dialog.md` to define the default and stream-switch behavior, and its use of the existing queue data. Update `specifications/ui/GRT-graduation-restart.md` only to make clear that the new-start default does not change restart behavior.
