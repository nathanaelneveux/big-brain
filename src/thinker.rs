//! Thinkers are the "brain" of an entity. You attach Scorers to it, and the
//! Thinker picks the right Action to run based on the resulting Scores.

use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use bevy::{
    log::{
        tracing::{field, span, Span},
        Level,
    },
    prelude::*,
};

use crate::{
    actions::{self, ActionBuilder, ActionBuilderWrapper, ActionState},
    choices::{Choice, ChoiceBuilder},
    pickers::Picker,
    scorers::{Score, ScorerBuilder},
};

/// Wrapper for Actor entities. In terms of Scorers, Thinkers, and Actions,
/// this is the [`Entity`] actually _performing_ the action, rather than the
/// entity a Scorer/Thinker/Action is attached to. Generally, you will use
/// this entity when writing Queries for Action and Scorer systems.
#[derive(Debug, Clone, Component, Copy, Reflect)]
pub struct Actor(pub Entity);

#[derive(Debug, Clone, Copy, Reflect)]
pub struct Action(pub Entity);

impl Action {
    pub fn entity(&self) -> Entity {
        self.0
    }
}

#[derive(Debug, Clone, Component)]
pub struct ActionSpan {
    pub(crate) span: Span,
}

impl ActionSpan {
    pub(crate) fn new(action: Entity, label: Option<&str>) -> Self {
        let span = span!(
            Level::DEBUG,
            "action",
            ent = ?action,
            label = field::Empty,
        );
        if let Some(label) = label {
            span.record("label", label);
        }
        Self { span }
    }

    pub fn span(&self) -> &Span {
        &self.span
    }
}

#[derive(Debug, Clone, Copy, Reflect)]
pub struct Scorer(pub Entity);

#[derive(Debug, Clone, Component)]
pub struct ScorerSpan {
    pub(crate) span: Span,
}

impl ScorerSpan {
    pub(crate) fn new(scorer: Entity, label: Option<&str>) -> Self {
        let span = span!(
            Level::DEBUG,
            "scorer",
            ent = ?scorer,
            label = field::Empty,
        );

        if let Some(label) = label {
            span.record("label", label);
        }
        Self { span }
    }

    pub fn span(&self) -> &Span {
        &self.span
    }
}

/// The "brains" behind this whole operation. A `Thinker` is what glues
/// together `Actions` and `Scorers` and shapes larger, intelligent-seeming
/// systems.
///
/// Note: Thinkers are also Actions, so anywhere you can pass in an Action (or
/// [`ActionBuilder`]), you can pass in a Thinker (or [`ThinkerBuilder`]).
///
/// ### Example
///
/// ```
/// # use bevy::prelude::*;
/// # use big_brain::prelude::*;
/// # #[derive(Component, Debug)]
/// # struct Thirst(f32, f32);
/// # #[derive(Component, Debug)]
/// # struct Hunger(f32, f32);
/// # #[derive(Clone, Component, Debug, ScorerBuilder)]
/// # struct Thirsty;
/// # #[derive(Clone, Component, Debug, ScorerBuilder)]
/// # struct Hungry;
/// # #[derive(Clone, Component, Debug, ActionBuilder)]
/// # struct Drink;
/// # #[derive(Clone, Component, Debug, ActionBuilder)]
/// # struct Eat;
/// # #[derive(Clone, Component, Debug, ActionBuilder)]
/// # struct Meander;
/// pub fn init_entities(mut cmd: Commands) {
///     cmd.spawn((
///         Thirst(70.0, 2.0),
///         Hunger(50.0, 3.0),
///         Thinker::build()
///             .picker(FirstToScore::new(0.8))
///             .when(Thirsty, Drink)
///             .when(Hungry, Eat)
///             .otherwise(Meander),
///     ));
/// }
/// ```
#[derive(Component, Debug, Reflect)]
#[reflect(from_reflect = false)]
pub struct Thinker {
    #[reflect(ignore)]
    picker: Arc<dyn Picker>,
    #[reflect(ignore)]
    otherwise: Option<ActionBuilderWrapper>,
    #[reflect(ignore)]
    choices: Vec<Choice>,
    #[reflect(ignore)]
    current_action: Option<(Action, ActionBuilderWrapper)>,
    current_action_label: Option<Option<String>>,
    #[reflect(ignore)]
    span: Span,
    #[reflect(ignore)]
    scheduled_actions: VecDeque<ActionBuilderWrapper>,
}

impl Thinker {
    /// Make a new [`ThinkerBuilder`]. This is what you'll actually use to
    /// configure Thinker behavior.
    pub fn build() -> ThinkerBuilder {
        ThinkerBuilder::new()
    }

    pub fn schedule_action(&mut self, action: impl ActionBuilder + 'static) {
        self.scheduled_actions
            .push_back(ActionBuilderWrapper::new(Arc::new(action)));
    }
}

/// This is what you actually use to configure Thinker behavior. It's a plain
/// old [`ActionBuilder`], as well.
#[derive(Component, Clone, Debug, Default)]
pub struct ThinkerBuilder {
    picker: Option<Arc<dyn Picker>>,
    otherwise: Option<ActionBuilderWrapper>,
    choices: Vec<ChoiceBuilder>,
    label: Option<String>,
}

impl ThinkerBuilder {
    pub(crate) fn new() -> Self {
        Self {
            picker: None,
            otherwise: None,
            choices: Vec::new(),
            label: None,
        }
    }

    /// Define a [`Picker`](crate::pickers::Picker) for this Thinker.
    pub fn picker(mut self, picker: impl Picker + 'static) -> Self {
        self.picker = Some(Arc::new(picker));
        self
    }

    /// Define an [`ActionBuilder`](crate::actions::ActionBuilder) and
    /// [`ScorerBuilder`](crate::scorers::ScorerBuilder) pair.
    pub fn when(
        mut self,
        scorer: impl ScorerBuilder + 'static,
        action: impl ActionBuilder + 'static,
    ) -> Self {
        self.choices
            .push(ChoiceBuilder::new(Arc::new(scorer), Arc::new(action)));
        self
    }

    /// Default `Action` to execute if the `Picker` did not pick any of the
    /// given choices.
    pub fn otherwise(mut self, otherwise: impl ActionBuilder + 'static) -> Self {
        self.otherwise = Some(ActionBuilderWrapper::new(Arc::new(otherwise)));
        self
    }

    /// * Configures a label to use for the thinker when logging.
    pub fn label(mut self, label: impl AsRef<str>) -> Self {
        self.label = Some(label.as_ref().to_string());
        self
    }
}

impl ActionBuilder for ThinkerBuilder {
    fn build(&self, cmd: &mut Commands, action_ent: Entity, actor: Entity) {
        let span = span!(
            Level::DEBUG,
            "thinker",
            actor = ?actor,
        );
        let _guard = span.enter();
        debug!("Spawning Thinker.");
        let choices = self
            .choices
            .iter()
            .map(|choice| choice.build(cmd, actor, action_ent))
            .collect();
        std::mem::drop(_guard);
        cmd.entity(action_ent)
            .insert(Thinker {
                // TODO: reasonable default?...
                picker: self
                    .picker
                    .clone()
                    .expect("ThinkerBuilder must have a Picker"),
                otherwise: self.otherwise.clone(),
                choices,
                current_action: None,
                current_action_label: None,
                span,
                scheduled_actions: VecDeque::new(),
            })
            .insert(Name::new("Thinker"))
            .insert(ActionState::Requested);
    }

    fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
}

pub fn thinker_component_attach_system(
    mut cmd: Commands,
    q: Query<(Entity, &ThinkerBuilder), Without<HasThinker>>,
) {
    for (entity, thinker_builder) in q.iter() {
        let thinker = actions::spawn_action(thinker_builder, &mut cmd, entity);
        cmd.entity(entity).insert(HasThinker(thinker));
    }
}

pub fn thinker_component_detach_system(
    mut cmd: Commands,
    q: Query<(Entity, &HasThinker), Without<ThinkerBuilder>>,
) {
    for (actor, HasThinker(thinker)) in q.iter() {
        if let Ok(mut ent) = cmd.get_entity(*thinker) {
            ent.despawn();
        }
        cmd.entity(actor).remove::<HasThinker>();
    }
}

pub fn actor_gone_cleanup(
    mut cmd: Commands,
    actors: Query<&ThinkerBuilder>,
    q: Query<(Entity, &Actor)>,
) {
    for (child, Actor(actor)) in q.iter() {
        if actors.get(*actor).is_err() {
            // Actor is gone. Let's clean up.
            if let Ok(mut ent) = cmd.get_entity(child) {
                ent.despawn();
            }
        }
    }
}

#[derive(Component, Debug, Reflect)]
pub struct HasThinker(Entity);

impl HasThinker {
    pub fn entity(&self) -> Entity {
        self.0
    }
}

pub struct ThinkerIterations {
    index: usize,
    max_duration: Duration,
}
impl ThinkerIterations {
    pub fn new(max_duration: Duration) -> Self {
        Self {
            index: 0,
            max_duration,
        }
    }
}
impl Default for ThinkerIterations {
    fn default() -> Self {
        Self::new(Duration::from_millis(10))
    }
}

fn cloned_action_state(
    states: &mut Query<&mut ActionState>,
    entity: Entity,
) -> Option<ActionState> {
    states.get_mut(entity).ok().map(|state| state.clone())
}

fn set_action_state(
    states: &mut Query<&mut ActionState>,
    entity: Entity,
    next: ActionState,
) -> bool {
    if let Ok(mut state) = states.get_mut(entity) {
        *state = next;
        true
    } else {
        false
    }
}

pub fn thinker_system(
    mut cmd: Commands,
    mut iterations: Local<ThinkerIterations>,
    mut thinker_q: Query<(Entity, &Actor, &mut Thinker)>,
    scores: Query<&Score>,
    mut action_states: Query<&mut actions::ActionState>,
    action_spans: Query<&ActionSpan>,
    scorer_spans: Query<&ScorerSpan>,
) {
    let start = Instant::now();
    for (thinker_ent, Actor(actor), mut thinker) in thinker_q.iter_mut().skip(iterations.index) {
        iterations.index += 1;

        let thinker_state = match cloned_action_state(&mut action_states, thinker_ent) {
            Some(state) => state,
            None => {
                debug!(
                    "Thinker entity {:?} is missing ActionState, skipping.",
                    thinker_ent
                );
                continue;
            }
        };

        let thinker_span = thinker.span.clone();
        let _thinker_span_guard = thinker_span.enter();

        match thinker_state {
            ActionState::Init => {
                debug!("Initializing thinker.");
                let _ = set_action_state(&mut action_states, thinker_ent, ActionState::Requested);
            }
            ActionState::Requested => {
                debug!("Thinker requested. Starting execution.");
                let _ = set_action_state(&mut action_states, thinker_ent, ActionState::Executing);
            }
            ActionState::Success | ActionState::Failure => {}
            ActionState::Cancelled => {
                debug!("Thinker cancelled. Cleaning up.");
                if let Some(current) = &mut thinker.current_action {
                    debug!("Cancelling current action because thinker was cancelled.");
                    let state = match cloned_action_state(&mut action_states, current.0 .0) {
                        Some(state) => state,
                        None => {
                            debug!(
                                "Current action is missing ActionState; dropping action handle."
                            );
                            thinker.current_action = None;
                            continue;
                        }
                    };
                    match state {
                        ActionState::Success | ActionState::Failure => {
                            debug!("Action already wrapped up on its own. Cleaning up action in Thinker.");
                            if let Ok(mut ent) = cmd.get_entity(current.0 .0) {
                                ent.despawn();
                            }
                            thinker.current_action = None;
                        }
                        ActionState::Cancelled => {
                            debug!("Current action already cancelled.");
                        }
                        _ => {
                            debug!( "Action is still executing. Attempting to cancel it before wrapping up Thinker cancellation.");
                            if let Ok(action_span) = action_spans.get(current.0 .0) {
                                action_span.span.in_scope(|| {
                                    debug!("Parent thinker was cancelled. Cancelling action.");
                                });
                            }
                            let _ = set_action_state(
                                &mut action_states,
                                current.0 .0,
                                ActionState::Cancelled,
                            );
                        }
                    }
                } else {
                    debug!("No current thinker action. Wrapping up Thinker as Succeeded.");
                    let _ = set_action_state(&mut action_states, thinker_ent, ActionState::Success);
                }
            }
            ActionState::Executing => {
                #[cfg(feature = "trace")]
                trace!("Thinker is executing. Thinking...");
                if let Some(choice) = thinker.picker.pick(&thinker.choices, &scores) {
                    // Think about what action we're supposed to be taking. We do this
                    // every tick, because we might change our mind.
                    // ...and then execute it (details below).
                    #[cfg(feature = "trace")]
                    trace!("Action picked. Executing picked action.");
                    let action = choice.action.clone();
                    let scorer = choice.scorer;
                    let Ok(score) = scores.get(choice.scorer.0) else {
                        debug!(
                            "Picked scorer {:?} is missing Score; skipping tick.",
                            choice.scorer.0
                        );
                        continue;
                    };
                    exec_picked_action(
                        &mut cmd,
                        *actor,
                        &mut thinker,
                        &action,
                        &mut action_states,
                        &action_spans,
                        Some((&scorer, score)),
                        &scorer_spans,
                        true,
                    );
                } else if should_schedule_action(&mut thinker, &mut action_states) {
                    debug!("Spawning scheduled action.");
                    if let Some(action) = thinker.scheduled_actions.pop_front() {
                        let new_action = actions::spawn_action(action.1.as_ref(), &mut cmd, *actor);
                        thinker.current_action = Some((Action(new_action), action.clone()));
                        thinker.current_action_label = Some(action.1.label().map(|s| s.into()));
                    }
                } else if let Some(default_action_ent) = &thinker.otherwise {
                    // Otherwise, let's just execute the default one! (if it's there)
                    let default_action_ent = default_action_ent.clone();
                    exec_picked_action(
                        &mut cmd,
                        *actor,
                        &mut thinker,
                        &default_action_ent,
                        &mut action_states,
                        &action_spans,
                        None,
                        &scorer_spans,
                        false,
                    );
                } else if let Some((action_ent, _)) = &thinker.current_action {
                    let curr_action_state = cloned_action_state(&mut action_states, action_ent.0);
                    let Some(curr_action_state) = curr_action_state else {
                        debug!(
                            "Current action {:?} has no ActionState; clearing handle.",
                            action_ent.0
                        );
                        thinker.current_action = None;
                        continue;
                    };
                    let previous_done = matches!(
                        curr_action_state,
                        ActionState::Success | ActionState::Failure
                    );
                    let _guard = action_spans
                        .get(action_ent.0)
                        .ok()
                        .map(|action_span| action_span.span.enter());
                    if previous_done {
                        debug!(
                            "Action completed and nothing was picked. Despawning action entity.",
                        );
                        // Despawn the action itself.
                        if let Ok(mut ent) = cmd.get_entity(action_ent.0) {
                            ent.despawn();
                        }
                        thinker.current_action = None;
                    } else if curr_action_state == ActionState::Init {
                        let _ = set_action_state(
                            &mut action_states,
                            action_ent.0,
                            ActionState::Requested,
                        );
                    }
                }
            }
        }
        if iterations.index.is_multiple_of(500) && start.elapsed() > iterations.max_duration {
            return;
        }
    }
    iterations.index = 0;
}

fn should_schedule_action(
    thinker: &mut Mut<Thinker>,
    states: &mut Query<&mut ActionState>,
) -> bool {
    #[cfg(feature = "trace")]
    let thinker_span = thinker.span.clone();
    #[cfg(feature = "trace")]
    let _thinker_span_guard = thinker_span.enter();
    if thinker.scheduled_actions.is_empty() {
        #[cfg(feature = "trace")]
        trace!("No scheduled actions. Not scheduling anything.");
        false
    } else if let Some((action_ent, _)) = &mut thinker.current_action {
        let Some(curr_action_state) = cloned_action_state(states, action_ent.0) else {
            debug!(
                "Current action {:?} missing ActionState; allowing scheduled action.",
                action_ent.0
            );
            return true;
        };

        let action_done = matches!(
            curr_action_state,
            ActionState::Success | ActionState::Failure
        );

        #[cfg(feature = "trace")]
        if action_done {
            trace!("Current action is already done. Can schedule.");
        } else {
            trace!("Current action is still executing. Not scheduling anything.");
        }

        action_done
    } else {
        #[cfg(feature = "trace")]
        trace!("No current action actions. Can schedule.");
        true
    }
}

#[allow(clippy::too_many_arguments)]
fn exec_picked_action(
    cmd: &mut Commands,
    actor: Entity,
    thinker: &mut Mut<Thinker>,
    picked_action: &ActionBuilderWrapper,
    states: &mut Query<&mut ActionState>,
    action_spans: &Query<&ActionSpan>,
    scorer_info: Option<(&Scorer, &Score)>,
    scorer_spans: &Query<&ScorerSpan>,
    override_current: bool,
) {
    // If we do find one, then we need to grab the corresponding
    // component for it. The "action" that `picker.pick()` returns
    // is just a newtype for an Entity.
    //

    // Now we check the current action. We need to check if we picked the same one as the previous tick.
    //
    // TODO: I don't know where the right place to put this is
    // (maybe not in this logic), but we do need some kind of
    // oscillation protection so we're not just bouncing back and
    // forth between the same couple of actions.
    let thinker_span = thinker.span.clone();
    let _thinker_span_guard = thinker_span.enter();
    if let Some((action_ent, ActionBuilderWrapper(current_id, _))) = &mut thinker.current_action {
        let Some(curr_action_state) = cloned_action_state(states, action_ent.0) else {
            debug!(
                "Current action {:?} missing ActionState; resetting current action.",
                action_ent.0
            );
            thinker.current_action = None;
            return;
        };
        let previous_done = matches!(
            curr_action_state,
            ActionState::Success | ActionState::Failure
        );
        let _guard = action_spans
            .get(action_ent.0)
            .ok()
            .map(|action_span| action_span.span.enter());
        if (!Arc::ptr_eq(current_id, &picked_action.0) && override_current) || previous_done {
            // So we've picked a different action than we were
            // currently executing. Just like before, we grab the
            // actual Action component (and we assume it exists).
            // If the action is executing, or was requested, we
            // need to cancel it to make sure it stops.
            if !previous_done {
                if override_current {
                    #[cfg(feature = "trace")]
                    trace!("Falling back to `otherwise` clause.",);
                } else {
                    #[cfg(feature = "trace")]
                    trace!("Picked a different action than the current one.",);
                }
            }
            match curr_action_state {
                ActionState::Executing | ActionState::Requested => {
                    debug!("Previous action is still executing. Requesting action cancellation.",);
                    let _ = set_action_state(states, action_ent.0, ActionState::Cancelled);
                }
                ActionState::Init | ActionState::Success | ActionState::Failure => {
                    debug!("Previous action already completed. Despawning action entity.",);
                    // Despawn the action itself.
                    if let Ok(mut ent) = cmd.get_entity(action_ent.0) {
                        ent.despawn();
                    }
                    if let Some((Scorer(ent), score)) = scorer_info {
                        if let Ok(scorer_span) = scorer_spans.get(*ent) {
                            let _guard = scorer_span.span.enter();
                            debug!("Winning scorer chosen with score {}", score.get());
                        } else {
                            debug!("Winning scorer chosen with score {}", score.get());
                        }
                    }
                    std::mem::drop(_guard);
                    debug!("Spawning next action");
                    let new_action =
                        Action(actions::spawn_action(picked_action.1.as_ref(), cmd, actor));
                    thinker.current_action = Some((new_action, picked_action.clone()));
                    thinker.current_action_label = Some(picked_action.1.label().map(|s| s.into()));
                }
                ActionState::Cancelled => {
                    #[cfg(feature = "trace")]
                    trace!(
                    "Cancellation already requested. Waiting for action to be marked as completed.",
                )
                }
            };
        } else {
            // Otherwise, it turns out we want to keep executing
            // the same action. Just in case, we go ahead and set
            // it as Requested if for some reason it had finished
            // but the Action System hasn't gotten around to
            // cleaning it up.
            if curr_action_state == ActionState::Init {
                let _ = set_action_state(states, action_ent.0, ActionState::Requested);
            }
            #[cfg(feature = "trace")]
            trace!("Continuing execution of current action.",)
        }
    } else {
        #[cfg(feature = "trace")]
        trace!("Falling back to `otherwise` clause.",);

        // This branch arm is called when there's no
        // current_action in the thinker. The logic here is pretty
        // straightforward -- we set the action, Request it, and
        // that's it.
        if let Some((Scorer(ent), score)) = scorer_info {
            if let Ok(scorer_span) = scorer_spans.get(*ent) {
                let _guard = scorer_span.span.enter();
                debug!("Winning scorer chosen with score {}", score.get());
            } else {
                debug!("Winning scorer chosen with score {}", score.get());
            }
        }
        debug!("No current action. Spawning new action.");
        let new_action = actions::spawn_action(picked_action.1.as_ref(), cmd, actor);
        thinker.current_action = Some((Action(new_action), picked_action.clone()));
        thinker.current_action_label = Some(picked_action.1.label().map(|s| s.into()));
    }
}
