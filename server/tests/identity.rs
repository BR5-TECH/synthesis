//! The identity, membership, invitation, and device tests.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.

mod common;

use common::{make_user, Api};
use synthesis_server::application::commands::{
    AcceptInvitation, CreateDevice, CreateGroup, CreateInvitation, CreateMembership, CreateUser,
    Page, UpdateMembership,
};
use synthesis_server::application::ports::Principal;
use synthesis_server::domain::identity::{InvitationState, Status, TargetType};
use synthesis_server::domain::permissions::MemberRole;

fn team(api: &Api, name: &str) -> synthesis_server::domain::identity::Team {
    api.application()
        .identity
        .create_team(
            &api.administrator(),
            CreateGroup {
                name: name.to_string(),
                ..Default::default()
            },
        )
        .expect("the team is written")
}

// SAS-FR-DTXV: the store holds the reconciled administrator and no other user
// when the service starts.
#[test]
fn a_new_service_holds_the_administrator_alone() {
    let api = Api::new();
    let users = api
        .application()
        .identity
        .list_users(&api.administrator(), Page::default())
        .expect("the users are read");
    assert_eq!(users.len(), 1);
    assert!(users[0].is_administrator());
    assert_eq!(users[0].id, api.administrator().user_id);
}

// SAS-FR-OZET: a create request may name the identifier, and a second record
// with that identifier is refused.
#[test]
fn a_duplicate_identifier_is_refused() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let id = uuid::Uuid::from_u128(7).to_string();

    application
        .identity
        .create_user(
            &admin,
            CreateUser {
                id: Some(id.clone()),
                display_name: "A user".to_string(),
                ..Default::default()
            },
        )
        .expect("the first user is written");
    let refused = application
        .identity
        .create_user(
            &admin,
            CreateUser {
                id: Some(id),
                display_name: "Another user".to_string(),
                ..Default::default()
            },
        )
        .expect_err("the second user is refused");
    assert_eq!(refused.code(), "duplicate_id");
}

// SAS-FR-TQIM: an empty field and an over-long field are refused, and the
// refusal names the field.
#[test]
fn a_field_that_breaks_a_limit_is_refused() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();

    for (name, email) in [
        ("   ", None),
        ("A user", Some("not-an-address".to_string())),
    ] {
        let refused = application
            .identity
            .create_user(
                &admin,
                CreateUser {
                    display_name: name.to_string(),
                    email,
                    ..Default::default()
                },
            )
            .expect_err("the field is refused");
        assert_eq!(refused.code(), "invalid_field");
    }
}

// SAS-FR-TWEL: the identity claim is unique, and the email is not the key.
#[test]
fn one_identity_claim_reaches_one_user() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();

    application
        .identity
        .create_user(
            &admin,
            CreateUser {
                display_name: "A user".to_string(),
                email: Some("person@example.com".to_string()),
                issuer: Some("https://issuer.example".to_string()),
                subject: Some("subject-1".to_string()),
                ..Default::default()
            },
        )
        .expect("the first user is written");

    let refused = application
        .identity
        .create_user(
            &admin,
            CreateUser {
                display_name: "Another user".to_string(),
                issuer: Some("https://issuer.example".to_string()),
                subject: Some("subject-1".to_string()),
                ..Default::default()
            },
        )
        .expect_err("a second user with one claim is refused");
    assert_eq!(refused.code(), "invalid_field");

    // Two users may hold one email address, because the email is a profile
    // attribute rather than the identity key.
    application
        .identity
        .create_user(
            &admin,
            CreateUser {
                display_name: "A third user".to_string(),
                email: Some("person@example.com".to_string()),
                ..Default::default()
            },
        )
        .expect("the email is no identity key");
}

// SAS-FR-MZQF: a membership is a record of its own, with its own identifier.
#[test]
fn a_membership_is_a_record_of_its_own() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let member = make_user(&application, &admin, "A member");
    let team = team(&api, "A team");

    let membership = application
        .memberships
        .create_membership(
            &admin,
            CreateMembership {
                member_id: member.to_string(),
                target_type: "team".to_string(),
                target_id: team.id.to_string(),
                role: "member".to_string(),
                ..Default::default()
            },
        )
        .expect("the membership is written");

    assert_eq!(membership.member_id, member);
    assert_eq!(membership.target_type, TargetType::Team);
    assert_eq!(membership.role, MemberRole::Member);
    assert_eq!(membership.status, Status::Active);
    assert_eq!(
        application
            .memberships
            .read_membership(&admin, membership.id)
            .expect("the membership is read")
            .id,
        membership.id
    );
}

// SAS-FR-BFHN: a second active membership for one member and one target is
// refused.
#[test]
fn a_duplicate_membership_is_refused() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let member = make_user(&application, &admin, "A member");
    let team = team(&api, "A team");

    let command = CreateMembership {
        member_id: member.to_string(),
        target_type: "team".to_string(),
        target_id: team.id.to_string(),
        role: "member".to_string(),
        ..Default::default()
    };
    application
        .memberships
        .create_membership(&admin, command.clone())
        .expect("the first membership");
    let refused = application
        .memberships
        .create_membership(&admin, command)
        .expect_err("the second membership is refused");
    assert_eq!(refused.code(), "duplicate_membership");
}

// SAS-FR-MZQF: a membership addresses a team or an organization, and a user is
// a member rather than a target.
#[test]
fn a_membership_target_is_never_a_user() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let member = make_user(&application, &admin, "A member");

    let refused = application
        .memberships
        .create_membership(
            &admin,
            CreateMembership {
                member_id: member.to_string(),
                target_type: "user".to_string(),
                target_id: member.to_string(),
                role: "member".to_string(),
                ..Default::default()
            },
        )
        .expect_err("a user is no target");
    assert_eq!(refused.code(), "invalid_field");
}

// SAS-FR-ATLB: an invitation is single-use, and an expired, revoked, or
// accepted invitation writes no membership.
#[test]
fn an_invitation_is_accepted_once() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let invitee = make_user(&application, &admin, "An invitee");
    let group = team(&api, "A team");

    let invitation = application
        .memberships
        .create_invitation(
            &admin,
            CreateInvitation {
                target_type: "team".to_string(),
                target_id: group.id.to_string(),
                invitee_user_id: Some(invitee.to_string()),
                role: "member".to_string(),
                expires_at: "2999-01-01T00:00:00Z".to_string(),
                ..Default::default()
            },
        )
        .expect("the invitation is written");
    assert_eq!(invitation.state, InvitationState::Pending);

    let actor = Principal::member(invitee);
    let membership = application
        .memberships
        .accept_invitation(&actor, invitation.id, AcceptInvitation::default())
        .expect("the invitee accepts");
    assert_eq!(membership.member_id, invitee);
    assert_eq!(
        application
            .memberships
            .read_invitation(&admin, invitation.id)
            .expect("the invitation is read")
            .state,
        InvitationState::Accepted
    );

    let refused = application
        .memberships
        .accept_invitation(&actor, invitation.id, AcceptInvitation::default())
        .expect_err("a second acceptance is refused");
    assert_eq!(refused.code(), "invitation_not_pending");
}

// SAS-FR-ATLB: an expired invitation and a revoked invitation are refused.
#[test]
fn an_expired_or_revoked_invitation_is_refused() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let invitee = make_user(&application, &admin, "An invitee");
    let group = team(&api, "A team");

    let expired = application
        .memberships
        .create_invitation(
            &admin,
            CreateInvitation {
                target_type: "team".to_string(),
                target_id: group.id.to_string(),
                invitee_user_id: Some(invitee.to_string()),
                role: "member".to_string(),
                expires_at: "2000-01-01T00:00:00Z".to_string(),
                ..Default::default()
            },
        )
        .expect("the invitation is written");
    let refused = application
        .memberships
        .accept_invitation(
            &Principal::member(invitee),
            expired.id,
            AcceptInvitation::default(),
        )
        .expect_err("an expired invitation is refused");
    assert_eq!(refused.code(), "invitation_not_pending");

    let revoked = application
        .memberships
        .create_invitation(
            &admin,
            CreateInvitation {
                target_type: "team".to_string(),
                target_id: group.id.to_string(),
                invitee_user_id: Some(invitee.to_string()),
                role: "member".to_string(),
                expires_at: "2999-01-01T00:00:00Z".to_string(),
                ..Default::default()
            },
        )
        .expect("the invitation is written");
    application
        .memberships
        .revoke_invitation(&admin, revoked.id)
        .expect("the invitation is revoked");
    let refused = application
        .memberships
        .accept_invitation(
            &Principal::member(invitee),
            revoked.id,
            AcceptInvitation::default(),
        )
        .expect_err("a revoked invitation is refused");
    assert_eq!(refused.code(), "invitation_not_pending");

    assert!(
        application
            .memberships
            .list_memberships(&admin, Default::default())
            .expect("the memberships are read")
            .is_empty(),
        "a refused acceptance writes no membership"
    );
}

// SAS-FR-ATLB: an acceptance that cannot write the membership leaves the
// invitation pending, so it may be repeated.
#[test]
fn a_refused_acceptance_leaves_the_invitation_pending() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let invitee = make_user(&application, &admin, "An invitee");
    let group = team(&api, "A team");

    application
        .memberships
        .create_membership(
            &admin,
            CreateMembership {
                member_id: invitee.to_string(),
                target_type: "team".to_string(),
                target_id: group.id.to_string(),
                role: "member".to_string(),
                ..Default::default()
            },
        )
        .expect("the member already belongs to the team");

    let invitation = application
        .memberships
        .create_invitation(
            &admin,
            CreateInvitation {
                target_type: "team".to_string(),
                target_id: group.id.to_string(),
                invitee_user_id: Some(invitee.to_string()),
                role: "member".to_string(),
                expires_at: "2999-01-01T00:00:00Z".to_string(),
                ..Default::default()
            },
        )
        .expect("the invitation is written");

    let refused = application
        .memberships
        .accept_invitation(
            &Principal::member(invitee),
            invitation.id,
            AcceptInvitation::default(),
        )
        .expect_err("the membership is a duplicate");
    assert_eq!(refused.code(), "duplicate_membership");
    assert_eq!(
        application
            .memberships
            .read_invitation(&admin, invitation.id)
            .expect("the invitation is read")
            .state,
        InvitationState::Pending
    );
}

// SAS-FR-SGXQ: a user that owns a team is not deleted while the team stands.
#[test]
fn a_referenced_record_is_not_deleted() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let owner = make_user(&application, &admin, "The owner");

    application
        .identity
        .create_team(
            &Principal::member(owner),
            CreateGroup {
                name: "A team".to_string(),
                ..Default::default()
            },
        )
        .expect("the owner writes a team");

    let refused = application
        .identity
        .delete_user(&admin, owner)
        .expect_err("the owner is referenced");
    assert_eq!(refused.code(), "referenced");
}

// SAS-FR-FQVS: a device holds its own stable identifier and a revocation state.
#[test]
fn a_device_is_registered_and_revoked() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let owner = make_user(&application, &admin, "The owner");
    let actor = Principal::member(owner);

    let device = application
        .memberships
        .create_device(
            &actor,
            CreateDevice {
                device_type: "ide".to_string(),
                display_name: "The workstation".to_string(),
                ..Default::default()
            },
        )
        .expect("the device is registered");
    assert_eq!(device.owner_id, owner);
    assert!(!device.is_revoked());

    let revoked = application
        .memberships
        .revoke_device(&actor, device.id)
        .expect("the device is revoked");
    assert!(revoked.is_revoked());
    assert_eq!(revoked.id, device.id, "the identifier is stable");

    // Another user reaches neither the record nor its revocation.
    let stranger = make_user(&application, &admin, "A stranger");
    assert_eq!(
        application
            .memberships
            .read_device(&Principal::member(stranger), device.id)
            .expect_err("a stranger reads no device")
            .code(),
        "forbidden"
    );
}

// SAS-FR-EPXN: a membership role outside the set is refused.
#[test]
fn an_unknown_membership_role_is_refused() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let member = make_user(&application, &admin, "A member");
    let group = team(&api, "A team");

    let refused = application
        .memberships
        .create_membership(
            &admin,
            CreateMembership {
                member_id: member.to_string(),
                target_type: "team".to_string(),
                target_id: group.id.to_string(),
                role: "chief".to_string(),
                ..Default::default()
            },
        )
        .expect_err("an unknown role is refused");
    assert_eq!(refused.code(), "invalid_role");

    let membership = application
        .memberships
        .create_membership(
            &admin,
            CreateMembership {
                member_id: member.to_string(),
                target_type: "team".to_string(),
                target_id: group.id.to_string(),
                role: "member".to_string(),
                ..Default::default()
            },
        )
        .expect("a known role is accepted");
    assert_eq!(
        application
            .memberships
            .update_membership(
                &admin,
                membership.id,
                UpdateMembership {
                    role: Some("chief".to_string()),
                    ..Default::default()
                },
            )
            .expect_err("an unknown role is refused on an update")
            .code(),
        "invalid_role"
    );
}
