use super::*;

/// Inspect before accepting either CLI or streamed completion. Failed security
/// claims trigger a bounded correction; logs alone are not an acceptance gate.
impl Agent {
    pub(super) fn continue_unverified_security_report(
        &mut self,
        attempts: &mut u32,
    ) -> Result<bool> {
        let Some(state) = crate::engagement_runtime::read(&self.session.id)? else {
            return Ok(false);
        };
        if state.exchanges.is_empty() && state.research.investigation.leads().is_empty() {
            return Ok(false);
        }
        let report = self.last_assistant_text().unwrap_or_default();
        let mut evidence =
            super::claim_checker::SessionEvidence::from_messages(&self.session.messages);
        evidence.exchanges.extend(state.exchanges);
        evidence.proofs.extend(state.proofs.into_values());
        let cwd = self.working_dir().map(PathBuf::from).unwrap_or_default();
        let results = super::claim_checker::check_all_claims(&report, &cwd, &evidence);
        let failures: Vec<_> = results
            .into_iter()
            .filter(|result| {
                !result.verified
                    && matches!(
                        result.claim.claim_type,
                        super::claim_checker::ClaimType::VulnerabilityConfirmed(_)
                            | super::claim_checker::ClaimType::SecurityAbsence
                    )
            })
            .collect();
        if failures.is_empty() {
            return Ok(false);
        }
        if *attempts >= 2 {
            anyhow::bail!(
                "Security report rejected: unsupported vulnerability or class-wide absence claims remain after two correction attempts. Findings are not verified; inspect the engagement evidence."
            );
        }
        *attempts += 1;
        let details = failures
            .iter()
            .take(8)
            .map(|result| result.evidence.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        self.add_message(Role::User,vec![ContentBlock::Text {
            text:format!("<system-reminder>Security report acceptance failed. {details}\nCorrect or qualify unsupported claims, or obtain the missing permitted evidence through investigation. Distinguish observed effects, independent impact review, untested coverage and blocked work. Do not invent proof or expand scope.</system-reminder>"),cache_control:None,
        }]);
        Ok(true)
    }
}
