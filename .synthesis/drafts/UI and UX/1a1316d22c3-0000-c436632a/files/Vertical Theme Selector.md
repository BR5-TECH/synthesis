## Intent

Theme selector in the top chrome of the IDE is horizontal, making 3 buttons to select the theme, and then a useless button for “history” which does nothing and placeholder button for user’s profile which does nothing. We should leave only 2 buttons: agents button and the theme selector.

The theme selector becomes one trigger button. The trigger opens a vertical dropdown with one row per theme: light, dark, system. The three horizontal buttons are removed. The history button and the user profile placeholder button are removed from the top chrome. Nothing replaces them.

## User journey

- User opens a project in Synthesis IDE
- User sees 2 buttons in the right corner of the top chrome: agents and theme selector.
- The trigger of the theme selector shows the icon of the currently selected theme.
- WHEN: User clicks on the trigger of the theme selector, the dropdown opens under the trigger. It shows three rows in a vertical list, in the order light, dark, system. Each row shows an icon only, with no text. The row of the current theme is clearly marked as selected.
- WHEN: User clicks on a row in the dropdown, the theme changes at once, the dropdown closes, and the trigger icon updates to the new theme.
- WHEN: User presses Escape, or clicks outside the dropdown, the dropdown closes and the theme does not change.
- WHEN: User works with the keyboard only: Tab moves focus to the trigger. Enter, Space, or Arrow Down opens the dropdown and puts focus on the selected row. Arrow Up and Arrow Down move focus between rows. Enter selects the focused row. Escape closes the dropdown and returns focus to the trigger.

## Requirements

- Behavior of the Agents button stays as is.
- Size of the buttons stays as is.
- The tooltip on mouse over that the theme selector has today stays as is, on the trigger. Each dropdown row has the tooltip that its button has today. Tooltips show on hover and on keyboard focus.
- The icon for "system" is not the gear icon, because the gear icon is the icon of settings. Use an icon that shows a computer display. Light and dark keep their current icons.
- The right corner of the top chrome holds exactly two controls, in this order: the agents control, then the theme selector (`SNV-FR-59`). The footprint of the trigger does not change when the selected theme changes.
- The theme selector keeps the behavior of `SNV-FR-15` and `SNV-FR-16`: it reads and writes the one shared user-global theme preference through "load app preferences" and "save app preferences", and it reflects a change made in Global settings. Choosing a theme applies it to the application root immediately.
- The dropdown is a floating overlay and follows `SNV-FR-56`: no other floating overlay is open at the same time.
- Accessibility: the trigger has an accessible name that states the current theme, and it declares that it opens a popup and whether it is open. The dropdown has `role="radiogroup"` and each row has `role="radio"` with `aria-checked`, and an accessible name that is the theme name (the rows show no text). Focus is always visible. The icons are legible in light and in dark.
- Specifications to change: `specifications/ui/SNV-shell-navigation.md` (`SNV-FR-15` describes the selector as a control with three choices, and `SNV-FR-59` places the controls of the trailing cluster) and, if it describes the selector control, `specifications/ui/GLS-global-settings.md` (`GLS-FR-05`). The Global settings Appearance selector does not change.