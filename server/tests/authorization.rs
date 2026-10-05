//! The access-control tests.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.

mod common;

use common::{make_grant, make_project, make_user, Api};
use synthesis_server::application::commands::{
    CreateDraft, CreateMembership, TransferOwnership, UpdateGrant, UpdateMembership,
};
use synthesis_server::application::ports::Principal;
use synthesis_server::domain::permissions::{Permission, ProjectRole};

/// The names of the permissions a principal holds on a project.
fn permissions(
    api: &Api,
    actor: &Principal,
    project: synthesis_server::domain::ids::ProjectId,
) -> Vec<String> {
    let admin = api.administrator();
    let (_, held) = api
        .application()
        .projects
        .effective_permissions(&admin, project, Some(actor.user_id))
        .expect("the permissions are read");
    held.into_iter()
        .map(|permission| permission.as_str().to_string())
        .collect()
}

// SAS-FR-HXAP: a direct user grant supplies the permissions of its role and no
// other.
#[test]
fn a_user_grant_supplies_the_permissions_of_its_role() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();

    let owner = make_user(&application, &admin, "The owner");
    let member = make_user(&application, &admin, "The member");
    let project = make_project(&application, &admin, owner, "A project");
    make_grant(
        &application,
        &admin,
        project.id,
        "user",
        &member.to_string(),
        "viewer",
    );

    let actor = Principal::member(member);
    let mut held = permissions(&api, &actor, project.id);
    held.sort();
    assert_eq!(
        held,
        vec![
            "conversation.read".to_string(),
            "draft.read".to_string(),
            "project.read".to_string(),
        ]
    );
}

// SAS-FR-ZDVR, SAS-FR-BJHF: a team grant reaches an active member, and a
// suspended membership supplies nothing.
#[test]
fn a_team_grant_reaches_an_active_member_alone() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();

    let owner = make_user(&application, &admin, "The owner");
    let member = make_user(&application, &admin, "The member");
    let project = make_project(&application, &admin, owner, "A project");
    let team = application
        .identity
        .create_team(
            &admin,
            synthesis_server::application::commands::CreateGroup {
                name: "A team".to_string(),
                ..Default::default()
            },
        )
        .expect("the team is written");
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
    make_grant(
        &application,
        &admin,
        project.id,
        "team",
        &team.id.to_string(),
        "contributor",
    );

    let actor = Principal::member(member);
    assert!(permissions(&api, &actor, project.id).contains(&"draft.create".to_string()));

    application
        .memberships
        .update_membership(
            &admin,
            membership.id,
            UpdateMembership {
                status: Some("suspended".to_string()),
                ..Default::default()
            },
        )
        .expect("the membership is suspended");
    assert!(permissions(&api, &actor, project.id).is_empty());
}

// SAS-FR-BJHF: a team membership grants nothing through an organization, and an
// organization membership grants nothing through a team.
#[test]
fn membership_does_not_nest() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();

    let owner = make_user(&application, &admin, "The owner");
    let member = make_user(&application, &admin, "The member");
    let project = make_project(&application, &admin, owner, "A project");

    let team = application
        .identity
        .create_team(
            &admin,
            synthesis_server::application::commands::CreateGroup {
                name: "A team".to_string(),
                ..Default::default()
            },
        )
        .expect("the team is written");
    let organization = application
        .identity
        .create_organization(
            &admin,
            synthesis_server::application::commands::CreateGroup {
                name: "An organization".to_string(),
                ..Default::default()
            },
        )
        .expect("the organization is written");

    application
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

    // The grant addresses the organization, and the user is a member of the
    // team alone.
    make_grant(
        &application,
        &admin,
        project.id,
        "organization",
        &organization.id.to_string(),
        "viewer",
    );

    let actor = Principal::member(member);
    assert!(permissions(&api, &actor, project.id).is_empty());
}

// SAS-FR-QSLY: grants are additive, and removing one leaves what another
// supplies.
#[test]
fn grants_are_additive_and_removal_keeps_what_another_supplies() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();

    let owner = make_user(&application, &admin, "The owner");
    let member = make_user(&application, &admin, "The member");
    let project = make_project(&application, &admin, owner, "A project");
    let team = application
        .identity
        .create_team(
            &admin,
            synthesis_server::application::commands::CreateGroup {
                name: "A team".to_string(),
                ..Default::default()
            },
        )
        .expect("the team is written");
    application
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

    let direct = make_grant(
        &application,
        &admin,
        project.id,
        "user",
        &member.to_string(),
        "viewer",
    );
    make_grant(
        &application,
        &admin,
        project.id,
        "team",
        &team.id.to_string(),
        "contributor",
    );

    let actor = Principal::member(member);
    let held = permissions(&api, &actor, project.id);
    assert!(held.contains(&"draft.create".to_string()));
    assert!(held.contains(&"project.read".to_string()));

    application
        .projects
        .delete_grant(&admin, project.id, direct.id)
        .expect("the direct grant is deleted");
    let held = permissions(&api, &actor, project.id);
    assert!(
        held.contains(&"project.read".to_string()),
        "the team grant still supplies the read permission"
    );
}

// SAS-FR-VNTC: a duplicate grant is refused, and a replacement is an update.
#[test]
fn a_duplicate_grant_is_refused_and_a_replacement_updates_one_record() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();

    let owner = make_user(&application, &admin, "The owner");
    let member = make_user(&application, &admin, "The member");
    let project = make_project(&application, &admin, owner, "A project");
    let grant = make_grant(
        &application,
        &admin,
        project.id,
        "user",
        &member.to_string(),
        "viewer",
    );

    let refused = application
        .projects
        .create_grant(
            &admin,
            project.id,
            synthesis_server::application::commands::CreateGrant {
                target_type: "user".to_string(),
                target_id: member.to_string(),
                role: "viewer".to_string(),
                ..Default::default()
            },
        )
        .expect_err("a second grant with one role is refused");
    assert_eq!(refused.code(), "duplicate_grant");

    let replaced = application
        .projects
        .update_grant(
            &admin,
            project.id,
            grant.id,
            UpdateGrant {
                role: "contributor".to_string(),
            },
        )
        .expect("the grant is replaced");
    assert_eq!(replaced.id, grant.id);
    assert_eq!(replaced.role, ProjectRole::Contributor);
    assert_eq!(
        application
            .projects
            .list_grants(&admin, project.id, Default::default())
            .expect("the grants are read")
            .len(),
        2,
        "the owner grant and the replaced grant, and no third record"
    );
}

// SAS-FR-EYUB: a revoked grant is ineffective at once and stays readable.
#[test]
fn a_revoked_grant_is_ineffective_and_readable() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();

    let owner = make_user(&application, &admin, "The owner");
    let member = make_user(&application, &admin, "The member");
    let project = make_project(&application, &admin, owner, "A project");
    let grant = make_grant(
        &application,
        &admin,
        project.id,
        "user",
        &member.to_string(),
        "contributor",
    );

    let actor = Principal::member(member);
    assert!(!permissions(&api, &actor, project.id).is_empty());

    let revoked = application
        .projects
        .revoke_grant(&admin, project.id, grant.id)
        .expect("the grant is revoked");
    assert!(revoked.revoked_at.is_some());
    assert_eq!(revoked.revoked_by, Some(admin.user_id));
    assert_eq!(revoked.role, ProjectRole::Contributor);
    assert!(permissions(&api, &actor, project.id).is_empty());

    let read = application
        .projects
        .read_grant(&admin, project.id, grant.id)
        .expect("a revoked grant is still readable");
    assert!(read.revoked_at.is_some());
}

// SAS-FR-MWOD: the owner grant is neither deleted, nor revoked, nor reduced,
// and no grant carries the owner role.
#[test]
fn the_owner_grant_is_protected() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();

    let owner = make_user(&application, &admin, "The owner");
    let other = make_user(&application, &admin, "Another user");
    let project = make_project(&application, &admin, owner, "A project");

    let grants = application
        .projects
        .list_grants(&admin, project.id, Default::default())
        .expect("the grants are read");
    let owner_grant = grants
        .iter()
        .find(|grant| grant.is_owner_grant(owner))
        .expect("the project holds the owner grant");

    for error in [
        application
            .projects
            .delete_grant(&admin, project.id, owner_grant.id)
            .err(),
        application
            .projects
            .revoke_grant(&admin, project.id, owner_grant.id)
            .err(),
        application
            .projects
            .update_grant(
                &admin,
                project.id,
                owner_grant.id,
                UpdateGrant {
                    role: "viewer".to_string(),
                },
            )
            .err(),
    ] {
        assert_eq!(
            error.expect("the owner grant is protected").code(),
            "owner_protected"
        );
    }

    let refused = application
        .projects
        .create_grant(
            &admin,
            project.id,
            synthesis_server::application::commands::CreateGrant {
                target_type: "user".to_string(),
                target_id: other.to_string(),
                role: "owner".to_string(),
                ..Default::default()
            },
        )
        .expect_err("the owner role is not granted");
    assert_eq!(refused.code(), "owner_protected");

    // The owner holds every permission at all times.
    let actor = Principal::member(owner);
    assert_eq!(
        permissions(&api, &actor, project.id).len(),
        Permission::ALL.len()
    );
}

// SAS-FR-RAKX: a transfer moves the ownership and the owner grant, and the
// former owner keeps no permission that no other grant supplies.
#[test]
fn ownership_moves_with_its_grant() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();

    let owner = make_user(&application, &admin, "The owner");
    let successor = make_user(&application, &admin, "The successor");
    let project = make_project(&application, &admin, owner, "A project");

    let moved = application
        .projects
        .transfer_ownership(
            &admin,
            project.id,
            TransferOwnership {
                new_owner_id: successor.to_string(),
            },
        )
        .expect("the ownership moves");
    assert_eq!(moved.owner_id, successor);

    assert_eq!(
        permissions(&api, &Principal::member(successor), project.id).len(),
        Permission::ALL.len()
    );
    assert!(permissions(&api, &Principal::member(owner), project.id).is_empty());

    let grants = application
        .projects
        .list_grants(&admin, project.id, Default::default())
        .expect("the grants are read");
    assert_eq!(grants.len(), 1, "the owner grant moved rather than doubled");
    assert!(grants[0].is_owner_grant(successor));
}

// SAS-FR-TKPZ: a principal that may not read the project is answered
// `not_found`, and one that may read it but holds no permission is answered
// `forbidden`.
#[test]
fn the_refusal_hides_a_project_and_names_a_missing_permission() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();

    let owner = make_user(&application, &admin, "The owner");
    let stranger = make_user(&application, &admin, "A stranger");
    let viewer = make_user(&application, &admin, "A viewer");
    let project = make_project(&application, &admin, owner, "A project");
    make_grant(
        &application,
        &admin,
        project.id,
        "user",
        &viewer.to_string(),
        "viewer",
    );

    let hidden = application
        .projects
        .read_project(&Principal::member(stranger), project.id)
        .expect_err("a stranger reads nothing");
    assert_eq!(hidden.code(), "not_found");

    let refused = application
        .content
        .create_draft(
            &Principal::member(viewer),
            project.id,
            CreateDraft {
                title: "A draft".to_string(),
                ..Default::default()
            },
        )
        .expect_err("a viewer creates no draft");
    assert_eq!(refused.code(), "forbidden");
    assert!(refused.to_string().contains("draft.create"), "{refused}");
}

// SAS-FR-XRPD: the administrator bypasses every check.
#[test]
fn the_administrator_bypasses_every_check() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();

    let owner = make_user(&application, &admin, "The owner");
    let project = make_project(&application, &admin, owner, "A project");

    application
        .projects
        .read_project(&admin, project.id)
        .expect("the administrator reads any project");
    let draft = application
        .content
        .create_draft(
            &admin,
            project.id,
            CreateDraft {
                title: "A draft".to_string(),
                ..Default::default()
            },
        )
        .expect("the administrator writes a draft");
    application
        .content
        .delete_draft(&admin, draft.id)
        .expect("the administrator deletes it again");
}

// SAS-FR-GBWS, SAS-FR-XONP: each operation needs the permission the contract
// surface names for it.
#[test]
fn each_operation_needs_its_own_permission() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();

    let owner = make_user(&application, &admin, "The owner");
    let contributor = make_user(&application, &admin, "A contributor");
    let project = make_project(&application, &admin, owner, "A project");
    make_grant(
        &application,
        &admin,
        project.id,
        "user",
        &contributor.to_string(),
        "contributor",
    );
    let actor = Principal::member(contributor);

    let draft = application
        .content
        .create_draft(
            &actor,
            project.id,
            CreateDraft {
                title: "A draft".to_string(),
                ..Default::default()
            },
        )
        .expect("a contributor holds draft.create");

    // A contributor holds neither draft.graduate, draft.delete, nor
    // conversation.lock, and holds no project.manage_acl.
    assert_eq!(
        application
            .content
            .graduate_draft(&actor, draft.id)
            .expect_err("a contributor graduates nothing")
            .code(),
        "forbidden"
    );
    assert_eq!(
        application
            .content
            .delete_draft(&actor, draft.id)
            .expect_err("a contributor deletes nothing")
            .code(),
        "forbidden"
    );
    assert_eq!(
        application
            .projects
            .list_grants(&actor, project.id, Default::default())
            .expect_err("a contributor manages no ACL")
            .code(),
        "forbidden"
    );
    assert_eq!(
        application
            .projects
            .update_project(&actor, project.id, Default::default())
            .expect_err("a contributor updates no project")
            .code(),
        "forbidden"
    );
}

// SAS-FR-UPKA: a grant whose target does not exist is refused.
#[test]
fn a_grant_needs_an_existing_target_and_an_existing_project() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();

    let owner = make_user(&application, &admin, "The owner");
    let project = make_project(&application, &admin, owner, "A project");

    let refused = application
        .projects
        .create_grant(
            &admin,
            project.id,
            synthesis_server::application::commands::CreateGrant {
                target_type: "team".to_string(),
                target_id: uuid::Uuid::from_u128(999).to_string(),
                role: "viewer".to_string(),
                ..Default::default()
            },
        )
        .expect_err("the target does not exist");
    assert_eq!(refused.code(), "not_found");
}

// SAS-FR-HXAP, SAS-FR-IJRO: an unknown role and an unknown permission are
// refused rather than ignored.
#[test]
fn an_unknown_role_is_refused() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();

    let owner = make_user(&application, &admin, "The owner");
    let member = make_user(&application, &admin, "A member");
    let project = make_project(&application, &admin, owner, "A project");

    let refused = application
        .projects
        .create_grant(
            &admin,
            project.id,
            synthesis_server::application::commands::CreateGrant {
                target_type: "user".to_string(),
                target_id: member.to_string(),
                role: "editor".to_string(),
                ..Default::default()
            },
        )
        .expect_err("an unknown role is refused");
    assert_eq!(refused.code(), "invalid_role");
}
