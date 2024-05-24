use std::marker::PhantomData;

use super::*;

pub(super) struct ProofTreeFormatter<'a, 'b, I> {
    f: &'a mut (dyn Write + 'b),
    _interner: PhantomData<I>,
}

enum IndentorState {
    StartWithNewline,
    OnNewline,
    Inline,
}

/// A formatter which adds 4 spaces of indentation to its input before
/// passing it on to its nested formatter.
///
/// We can use this for arbitrary levels of indentation by nesting it.
struct Indentor<'a, 'b> {
    f: &'a mut (dyn Write + 'b),
    state: IndentorState,
}

impl Write for Indentor<'_, '_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for line in s.split_inclusive('\n') {
            match self.state {
                IndentorState::StartWithNewline => self.f.write_str("\n    ")?,
                IndentorState::OnNewline => self.f.write_str("    ")?,
                IndentorState::Inline => {}
            }
            self.state =
                if line.ends_with('\n') { IndentorState::OnNewline } else { IndentorState::Inline };
            self.f.write_str(line)?;
        }

        Ok(())
    }
}

impl<'a, 'b, I: Interner> ProofTreeFormatter<'a, 'b, I> {
    pub(super) fn new(f: &'a mut (dyn Write + 'b)) -> Self {
        ProofTreeFormatter { f, _interner: PhantomData }
    }

    fn nested<F>(&mut self, func: F) -> std::fmt::Result
    where
        F: FnOnce(&mut ProofTreeFormatter<'_, '_, I>) -> std::fmt::Result,
    {
        write!(self.f, " {{")?;
        func(&mut ProofTreeFormatter {
            f: &mut Indentor { f: self.f, state: IndentorState::StartWithNewline },
            _interner: PhantomData,
        })?;
        writeln!(self.f, "}}")
    }

    pub(super) fn format_goal_evaluation(&mut self, eval: &GoalEvaluation<I>) -> std::fmt::Result {
        let goal_text = match eval.kind {
            GoalEvaluationKind::Root { orig_values: _ } => "ROOT GOAL",
            GoalEvaluationKind::Nested => "GOAL",
        };
        write!(self.f, "{}: {:?}", goal_text, eval.uncanonicalized_goal)?;
        self.nested(|this| this.format_canonical_goal_evaluation(&eval.evaluation))
    }

    pub(super) fn format_canonical_goal_evaluation(
        &mut self,
        eval: &CanonicalGoalEvaluation<I>,
    ) -> std::fmt::Result {
        writeln!(self.f, "GOAL: {:?}", eval.goal)?;

        match &eval.kind {
            CanonicalGoalEvaluationKind::Overflow => {
                writeln!(self.f, "OVERFLOW: {:?}", eval.result)
            }
            CanonicalGoalEvaluationKind::CycleInStack => {
                writeln!(self.f, "CYCLE IN STACK: {:?}", eval.result)
            }
            CanonicalGoalEvaluationKind::ProvisionalCacheHit => {
                writeln!(self.f, "PROVISIONAL CACHE HIT: {:?}", eval.result)
            }
            CanonicalGoalEvaluationKind::Evaluation { revisions } => {
                for (n, step) in revisions.iter().enumerate() {
                    write!(self.f, "REVISION {n}")?;
                    self.nested(|this| this.format_evaluation_step(step))?;
                }
                writeln!(self.f, "RESULT: {:?}", eval.result)
            }
        }
    }

    pub(super) fn format_evaluation_step(
        &mut self,
        evaluation_step: &GoalEvaluationStep<I>,
    ) -> std::fmt::Result {
        writeln!(self.f, "INSTANTIATED: {:?}", evaluation_step.instantiated_goal)?;
        self.format_probe(&evaluation_step.evaluation)
    }

    pub(super) fn format_probe(&mut self, probe: &Probe<I>) -> std::fmt::Result {
        match &probe.kind {
            ProbeKind::Root { result } => {
                write!(self.f, "ROOT RESULT: {result:?}")
            }
            ProbeKind::TryNormalizeNonRigid { result } => {
                write!(self.f, "TRY NORMALIZE NON-RIGID: {result:?}")
            }
            ProbeKind::NormalizedSelfTyAssembly => {
                write!(self.f, "NORMALIZING SELF TY FOR ASSEMBLY:")
            }
            ProbeKind::UnsizeAssembly => {
                write!(self.f, "ASSEMBLING CANDIDATES FOR UNSIZING:")
            }
            ProbeKind::UpcastProjectionCompatibility => {
                write!(self.f, "PROBING FOR PROJECTION COMPATIBILITY FOR UPCASTING:")
            }
            ProbeKind::OpaqueTypeStorageLookup { result } => {
                write!(self.f, "PROBING FOR AN EXISTING OPAQUE: {result:?}")
            }
            ProbeKind::TraitCandidate { source, result } => {
                write!(self.f, "CANDIDATE {source:?}: {result:?}")
            }
            ProbeKind::ShadowedEnvProbing => {
                write!(self.f, "PROBING FOR IMPLS SHADOWED BY PARAM-ENV CANDIDATE:")
            }
        }?;

        self.nested(|this| {
            for step in &probe.steps {
                match step {
                    ProbeStep::AddGoal(source, goal) => {
                        let source = match source {
                            GoalSource::Misc => "misc",
                            GoalSource::ImplWhereBound => "impl where-bound",
                            GoalSource::InstantiateHigherRanked => "higher-ranked goal",
                        };
                        writeln!(this.f, "ADDED GOAL ({source}): {goal:?}")?
                    }
                    ProbeStep::EvaluateGoals(eval) => this.format_added_goals_evaluation(eval)?,
                    ProbeStep::NestedProbe(probe) => this.format_probe(probe)?,
                    ProbeStep::MakeCanonicalResponse { shallow_certainty } => {
                        writeln!(this.f, "EVALUATE GOALS AND MAKE RESPONSE: {shallow_certainty:?}")?
                    }
                    ProbeStep::RecordImplArgs { impl_args } => {
                        writeln!(this.f, "RECORDED IMPL ARGS: {impl_args:?}")?
                    }
                }
            }
            Ok(())
        })
    }

    pub(super) fn format_added_goals_evaluation(
        &mut self,
        added_goals_evaluation: &AddedGoalsEvaluation<I>,
    ) -> std::fmt::Result {
        writeln!(self.f, "TRY_EVALUATE_ADDED_GOALS: {:?}", added_goals_evaluation.result)?;

        for (n, iterations) in added_goals_evaluation.evaluations.iter().enumerate() {
            write!(self.f, "ITERATION {n}")?;
            self.nested(|this| {
                for goal_evaluation in iterations {
                    this.format_goal_evaluation(goal_evaluation)?;
                }
                Ok(())
            })?;
        }

        Ok(())
    }
}
