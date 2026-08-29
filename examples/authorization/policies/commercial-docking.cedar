// Ground-control-managed use-case authorization for Waystation-1.
// Inputs are facts derived only after ACCESS cryptographic verification.

@id("authorize-commercial-session")
permit (
    principal is Vehicle,
    action == Action::"authorize_session",
    resource == Station::"waystation-1"
)
when {
    context.registered_vehicle &&
    context.docking_certified &&
    context.holder_proof_valid
};

@id("enter-approach")
permit (
    principal is Vehicle,
    action == Action::"enter_approach",
    resource == Station::"waystation-1"
)
when {
    context.registered_vehicle &&
    context.holder_proof_valid &&
    context.readiness_fresh &&
    context.initial_hold_confirmed &&
    context.retreat_available &&
    context.closing_rate_mmps <= 600
};

@id("enter-final-approach")
permit (
    principal is Vehicle,
    action == Action::"enter_final_approach",
    resource == Station::"waystation-1"
)
when {
    context.registered_vehicle &&
    context.docking_certified &&
    context.holder_proof_valid &&
    context.session_authorized &&
    context.readiness_fresh &&
    context.relative_navigation_valid &&
    context.approach_corridor_clear &&
    context.closing_rate_within_limit &&
    context.closing_rate_mmps <= 200 &&
    context.range_mm <= 1120
};

@id("engage-soft-capture")
permit (
    principal is Vehicle,
    action == Action::"engage_soft_capture",
    resource == Station::"waystation-1"
)
when {
    context.docking_certified &&
    context.holder_proof_valid &&
    context.session_authorized &&
    context.readiness_fresh &&
    context.alignment_within_limit &&
    context.capture_system_ready &&
    context.closing_rate_mmps <= 100 &&
    context.range_mm <= 320
};

@id("engage-hard-dock")
permit (
    principal is Vehicle,
    action == Action::"engage_hard_dock",
    resource == Station::"waystation-1"
)
when {
    context.docking_certified &&
    context.holder_proof_valid &&
    context.session_authorized &&
    context.readiness_fresh &&
    context.soft_capture_confirmed &&
    context.latches_ready &&
    context.relative_motion_stable &&
    context.closing_rate_mmps <= 50 &&
    context.range_mm <= 40
};
