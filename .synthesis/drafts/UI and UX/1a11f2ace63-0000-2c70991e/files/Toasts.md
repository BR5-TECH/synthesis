## Intent

Immediate messages about important events like graduations completions or errors should have convenient notifications beyond logs and blinking tabs.

## User journey

- User is working on a Draft
- IDE is in focus
- In background, there’s an ongoing graduation run 
- Eventually graduation run completes or requires user’s attention due to some issues or esacalations
- In-app notification is issued, and user sees a toast-like notification in the top right corner of the application window.
- Notification is either Info or Warn or Error level
- Click on notification toast brings user to the originator
- IF:  notification is issued by graduation run - click on toast opens graduation run.
- IF: notification is issued by change proposal or any other conversational agent interaction - click on toast opens corresponding discussion

## Requirements

- if IDE is not in focus - current OS notification mechanism is used.
- Toast notification mechanism should be a unified method integrated with IDE notification system. 