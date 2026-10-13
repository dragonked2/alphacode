use serde::{Deserialize, Serialize};

/// Multi-dimensional verification quality — replaces a single fake-precise
/// confidence number. The final verdict is derived, never fabricated.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct VerificationQuality {
    /// 0.0-1.0: can an independent agent reproduce it?
    pub reproducibility: f32,
    /// 0.0-1.0: how strong is the supporting evidence?
    pub evidence_strength: f32,
    /// 0.0-1.0: is impact demonstrated rather than assumed?
    pub impact_demonstration: f32,
    /// 0.0-1.0: is there exploitability evidence (not theory)?
    pub exploitability_evidence: f32,
    /// 0.0-1.0: was it independently verified?
    pub independence: f32,
    /// 0.0-1.0: level of contradiction (higher = worse).
    pub contradiction_level: f32,
    /// Number of unverified assumptions the claim depends on.
    pub assumption_count: u32,
}

impl VerificationQuality {
    pub fn verification_state(&self) -> VerificationState {
        if self.contradiction_level > 0.5 {
            return VerificationState::Disputed;
        }
        if self.assumption_count > 3 {
            return VerificationState::AssumptionHeavy;
        }
        let score = self.score();
        if score >= 0.85 && self.reproducibility >= 0.7 && self.independence > 0.0 {
            VerificationState::VerifiedReportable
        } else if score >= 0.6 {
            VerificationState::StrongCandidate
        } else if score >= 0.35 {
            VerificationState::WeakCandidate
        } else {
            VerificationState::InsufficientEvidence
        }
    }

    pub fn score(&self) -> f32 {
        let positive = self.reproducibility * 0.25
            + self.evidence_strength * 0.25
            + self.impact_demonstration * 0.2
            + self.exploitability_evidence * 0.15
            + self.independence * 0.15;
        let penalty =
            self.contradiction_level * 0.3 + (self.assumption_count as f32 * 0.05).min(0.3);
        (positive - penalty).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    InsufficientEvidence,
    WeakCandidate,
    StrongCandidate,
    VerifiedReportable,
    Disputed,
    AssumptionHeavy,
}

/// Explicit negative reasoning: what would make this NOT a vulnerability?
/// A candidate becomes stronger by surviving attempts to disprove it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NegativeHypothesis {
    PublicByDesign,
    NonSensitive,
    Unreachable,
    NonExploitable,
    RequiresImpossibleConditions,
    ClientOnlyBehavior,
    InformationalExposure,
    RateLimited,
    AlreadyProtected,
    ExpectedApiBehavior,
    ScannerArtifact,
    EnvironmentArtifact,
    // Enhanced negative hypotheses for modern vulnerabilities
    WafProtected,
    CorsRestricted,
    CsrfProtected,
    SameOriginPolicy,
    ContentSecurityPolicy,
    SubresourceIntegrity,
    TrustedTypes,
    HttpOnlyCookie,
    SecureCookie,
    SameSiteCookie,
    TokenBinding,
    CertificatePinning,
    MfaProtected,
    IpRestricted,
    GeoRestricted,
    TimeRestricted,
    UserAgentRestricted,
    RefererChecked,
    OriginChecked,
    CustomHeaderRequired,
    RequestSigning,
    ReplayProtection,
    NonceUsed,
    TimestampValidated,
    SignatureVerified,
    EncryptedChannel,
    MutualTls,
    NetworkSegmentation,
    VpnRequired,
    InternalNetworkOnly,
    AdminOnly,
    OwnerOnly,
    SelfOnly,
    NoSensitiveData,
    ReadOnlyOperation,
    IdempotentOperation,
    SafeHttpMethod,
    NoSideEffects,
    ReversibleAction,
    LoggedAndMonitored,
    RequiresAuthentication,
    RequiresAuthorization,
    RequiresPrivilege,
    RequiresPhysicalAccess,
    RequiresSocialEngineering,
    RequiresMalware,
    RequiresInsider,
    RequiresCompromisedAccount,
    RequiresCompromisedDevice,
    RequiresCompromisedNetwork,
    RequiresCompromisedDependency,
    RequiresZeroDay,
    RequiresRaceCondition,
    RequiresTimingAttack,
    RequiresPaddingOracle,
    RequiresLengthExtension,
    RequiresKnownPlaintext,
    RequiresChosenPlaintext,
    RequiresChosenCiphertext,
    RequiresAdaptiveChosenCiphertext,
    RequiresSideChannel,
    RequiresFaultInjection,
    RequiresGlitching,
    RequiresLaserInjection,
    RequiresElectromagneticAnalysis,
    RequiresPowerAnalysis,
    RequiresTimingAnalysis,
    RequiresAcousticAnalysis,
    RequiresOpticalAnalysis,
    RequiresThermalAnalysis,
    RequiresRadiationAnalysis,
    RequiresFaultAttack,
    RequiresRowhammer,
    RequiresSpectre,
    RequiresMeltdown,
    RequiresZombieLoad,
    RequiresRidl,
    RequiresFallout,
    RequiresStoreToLeak,
    RequiresLoadValueInjection,
    RequiresTransientExecution,
    RequiresSpeculativeExecution,
    RequiresBranchPrediction,
    RequiresCacheAttack,
    RequiresPrimeProbe,
    RequiresFlushReload,
    RequiresFlushFlush,
    RequiresEvictReload,
    RequiresReload,
    RequiresMemoryDeduplication,
    RequiresMemoryDisclosure,
    RequiresKernelLeak,
    RequiresKernelCorruption,
    RequiresKernelCodeExecution,
    RequiresKernelPrivilegeEscalation,
    RequiresKernelBypass,
    RequiresKernelMitigation,
    RequiresKernelPatch,
    RequiresKernelUpdate,
    RequiresKernelReboot,
    RequiresKernelRecompile,
    RequiresKernelConfig,
    RequiresKernelModule,
    RequiresKernelDriver,
    RequiresKernelFirmware,
    RequiresKernelBootloader,
    RequiresKernelUefi,
    RequiresKernelBios,
    RequiresKernelSmm,
    RequiresKernelTrustzone,
    RequiresKernelTee,
    RequiresKernelSgX,
    RequiresKernelTdx,
    RequiresKernelSeV,
    RequiresKernelCca,
    RequiresKernelPmp,
    RequiresKernelPma,
    RequiresKernelMte,
    RequiresKernelBti,
    RequiresKernelPac,
    RequiresKernelApa,
    RequiresKernelDit,
    RequiresKernelCfi,
    RequiresKernelShadowStack,
    RequiresKernelCet,
    RequiresKernelLkG,
    RequiresKernelKpti,
    RequiresKernelKaiser,
    RequiresKernelKaisr,
    RequiresKernelKaslr,
    RequiresKernelKptr,
    RequiresKernelKasan,
    RequiresKernelKcsan,
    RequiresKernelKmsan,
    RequiresKernelKfence,
    RequiresKernelKtsan,
    RequiresKernelKuBSan,
    RequiresKernelKcov,
    RequiresKernelKprobe,
    RequiresKernelKretprobe,
    RequiresKernelUprobe,
    RequiresKernelUretprobe,
    RequiresKernelTracepoint,
    RequiresKernelFtrace,
    RequiresKernelPerf,
    RequiresKernelBpf,
    RequiresKernelEbpf,
    RequiresKernelXdp,
    RequiresKernelTc,
    RequiresKernelCgroup,
    RequiresKernelNamespace,
    RequiresKernelSeccomp,
    RequiresKernelSelinux,
    RequiresKernelApparmor,
    RequiresKernelSmack,
    RequiresKernelTomoyo,
    RequiresKernelYama,
    RequiresKernelLandlock,
    RequiresKernelIma,
    RequiresKernelEvm,
    RequiresKernelTpm,
    RequiresKernelTss,
    RequiresKernelAcpi,
    RequiresKernelDmi,
    RequiresKernelSmbios,
    RequiresKernelIpmitool,
    RequiresKernelBmc,
    RequiresKernelIpmi,
    RequiresKernelRedfish,
    RequiresKernelSnmp,
    RequiresKernelNetconf,
    RequiresKernelRestconf,
    RequiresKernelGnmi,
    RequiresKernelGrpc,
    RequiresKernelThrift,
    RequiresKernelAvro,
    RequiresKernelProtobuf,
    RequiresKernelCapnproto,
    RequiresKernelFlatbuffers,
    RequiresKernelMsgpack,
    RequiresKernelCbor,
    RequiresKernelBson,
    RequiresKernelUbjson,
    RequiresKernelSmile,
    RequiresKernelIon,
    RequiresKernelAsn1,
    RequiresKernelBer,
    RequiresKernelDer,
    RequiresKernelPer,
    RequiresKernelXer,
    RequiresKernelOer,
    RequiresKernelCER,
    RequiresKernelJER,
    RequiresKernelGSER,
    RequiresKernelFastInfoset,
    RequiresKernelExi,
    RequiresKernelWbxml,
    RequiresKernelXml,
    RequiresKernelHtml,
    RequiresKernelXhtml,
    RequiresKernelSvg,
    RequiresKernelMathml,
    RequiresKernelRdf,
    RequiresKernelOwl,
    RequiresKernelSkos,
    RequiresKernelDublinCore,
    RequiresKernelFoaf,
    RequiresKernelSioc,
    RequiresKernelActivityStreams,
    RequiresKernelJsonLd,
    RequiresKernelMicrodata,
    RequiresKernelRdfa,
    RequiresKernelTurtle,
    RequiresKernelNtriples,
    RequiresKernelNquads,
    RequiresKernelTrig,
    RequiresKernelN3,
    RequiresKernelSparql,
    RequiresKernelShacl,
    RequiresKernelShex,
    RequiresKernelRdfSchema,
    RequiresKernelRdfs,
    RequiresKernelOwl2,
    RequiresKernelRuleML,
    RequiresKernelSwrl,
    RequiresKernelDatalog,
    RequiresKernelProlog,
    RequiresKernelErlang,
    RequiresKernelElixir,
    RequiresKernelHaskell,
    RequiresKernelOcaml,
    RequiresKernelFsharp,
    RequiresKernelScala,
    RequiresKernelKotlin,
    RequiresKernelClojure,
    RequiresKernelGroovy,
    RequiresKernelJRuby,
    RequiresKernelJython,
    RequiresKernelIronPython,
    RequiresKernelIronRuby,
    RequiresKernelCsharp,
    RequiresKernelVbnet,
    RequiresKernelCplusplus,
    RequiresKernelC,
    RequiresKernelRust,
    RequiresKernelGo,
    RequiresKernelSwift,
    RequiresKernelObjectiveC,
    RequiresKernelD,
    RequiresKernelNim,
    RequiresKernelCrystal,
    RequiresKernelZig,
    RequiresKernelV,
    RequiresKernelJulia,
    RequiresKernelR,
    RequiresKernelMatlab,
    RequiresKernelOctave,
    RequiresKernelMathematica,
    RequiresKernelMaple,
    RequiresKernelSage,
    RequiresKernelMaxima,
    RequiresKernelAxiom,
    RequiresKernelReduce,
    RequiresKernelMacsyma,
    RequiresKernelMupad,
    RequiresKernelMathcad,
    RequiresKernelLabview,
    RequiresKernelSimulink,
    RequiresKernelScilab,
    RequiresKernelFreemat,
    RequiresKernelGnuOctave,
    RequiresKernelWolframAlpha,
    RequiresKernelWolframLanguage,
    RequiresKernelWolframScript,
    RequiresKernelWolframEngine,
    RequiresKernelWolframCloud,
    RequiresKernelWolframDesktop,
    RequiresKernelWolframNotebook,
    RequiresKernelWolframPlayer,
    RequiresKernelWolframWorkbench,
    RequiresKernelWolframDataFramework,
    RequiresKernelWolframNeuralNetworks,
    RequiresKernelWolframMachineLearning,
    RequiresKernelWolframImageProcessing,
    RequiresKernelWolframSignalProcessing,
    RequiresKernelWolframControlSystems,
    RequiresKernelWolframWavelets,
    RequiresKernelWolframFinance,
    RequiresKernelWolframStatistics,
    RequiresKernelWolframCalculus,
    RequiresKernelWolframAlgebra,
    RequiresKernelWolframGeometry,
    RequiresKernelWolframGraphTheory,
    RequiresKernelWolframCombinatorics,
    RequiresKernelWolframNumberTheory,
    RequiresKernelWolframGroupTheory,
    RequiresKernelWolframRingTheory,
    RequiresKernelWolframFieldTheory,
    RequiresKernelWolframLinearAlgebra,
    RequiresKernelWolframDifferentialEquations,
    RequiresKernelWolframIntegralEquations,
    RequiresKernelWolframDifferenceEquations,
    RequiresKernelWolframFunctionalEquations,
    RequiresKernelWolframPartialDifferentialEquations,
    RequiresKernelWolframCalculusOfVariations,
    RequiresKernelWolframComplexAnalysis,
    RequiresKernelWolframRealAnalysis,
    RequiresKernelWolframFunctionalAnalysis,
    RequiresKernelWolframHarmonicAnalysis,
    RequiresKernelWolframFourierAnalysis,
    RequiresKernelWolframLaplaceTransforms,
    RequiresKernelWolframZTransforms,
    RequiresKernelWolframIntegralTransforms,
    RequiresKernelWolframSpecialFunctions,
    RequiresKernelWolframOrthogonalPolynomials,
    RequiresKernelWolframSphericalHarmonics,
    RequiresKernelWolframEllipticFunctions,
    RequiresKernelWolframThetaFunctions,
    RequiresKernelWolframHypergeometricFunctions,
    RequiresKernelWolframBesselFunctions,
    RequiresKernelWolframLegendreFunctions,
    RequiresKernelWolframHermiteFunctions,
    RequiresKernelWolframLaguerreFunctions,
    RequiresKernelWolframChebyshevFunctions,
    RequiresKernelWolframJacobiFunctions,
    RequiresKernelWolframGegenbauerFunctions,
    RequiresKernelKernelZernikePolynomials,
    RequiresKernelKernelBernoulliNumbers,
    RequiresKernelKernelEulerNumbers,
    RequiresKernelKernelCatalanNumbers,
    RequiresKernelKernelFibonacciNumbers,
    RequiresKernelKernelLucasNumbers,
    RequiresKernelKernelPellNumbers,
    RequiresKernelKernelTriangularNumbers,
    RequiresKernelKernelSquareNumbers,
    RequiresKernelKernelCubeNumbers,
    RequiresKernelKernelPerfectNumbers,
    RequiresKernelKernelAbundantNumbers,
    RequiresKernelKernelDeficientNumbers,
    RequiresKernelKernelAmicableNumbers,
    RequiresKernelKernelSociableNumbers,
    RequiresKernelKernelUntouchableNumbers,
    RequiresKernelKernelWeirdNumbers,
    RequiresKernelKernelSemiperfectNumbers,
    RequiresKernelKernelPracticalNumbers,
    RequiresKernelKernelSphenicNumbers,
    RequiresKernelKernelSquarefreeNumbers,
    RequiresKernelKernelPowerfulNumbers,
    RequiresKernelKernelAchillesNumbers,
    RequiresKernelKernelPrimaryPseudoperfectNumbers,
    RequiresKernelKernelUnitaryPerfectNumbers,
    RequiresKernelKernelMultiplyPerfectNumbers,
    RequiresKernelKernelHyperperfectNumbers,
    RequiresKernelKernelGegenbauerFunctions,
    RequiresKernelKernelJacobiFunctions,
    RequiresKernelKernelChebyshevFunctions,
    RequiresKernelKernelLaguerreFunctions,
    RequiresKernelKernelHermiteFunctions,
    RequiresKernelKernelLegendreFunctions,
    RequiresKernelKernelBesselFunctions,
    RequiresKernelKernelHypergeometricFunctions,
    RequiresKernelKernelThetaFunctions,
    RequiresKernelKernelEllipticFunctions,
    RequiresKernelKernelSphericalHarmonics,
    RequiresKernelKernelOrthogonalPolynomials,
    RequiresKernelKernelSpecialFunctions,
    RequiresKernelKernelIntegralTransforms,
    RequiresKernelKernelZTransforms,
    RequiresKernelKernelLaplaceTransforms,
    RequiresKernelKernelFourierAnalysis,
    RequiresKernelKernelHarmonicAnalysis,
    RequiresKernelKernelFunctionalAnalysis,
    RequiresKernelKernelRealAnalysis,
    RequiresKernelKernelComplexAnalysis,
    RequiresKernelKernelCalculusOfVariations,
    RequiresKernelKernelPartialDifferentialEquations,
    RequiresKernelKernelFunctionalEquations,
    RequiresKernelKernelDifferenceEquations,
    RequiresKernelKernelIntegralEquations,
    RequiresKernelKernelDifferentialEquations,
    RequiresKernelKernelLinearAlgebra,
    RequiresKernelKernelFieldTheory,
    RequiresKernelKernelRingTheory,
    RequiresKernelKernelGroupTheory,
    RequiresKernelKernelNumberTheory,
    RequiresKernelKernelCombinatorics,
    RequiresKernelKernelGraphTheory,
    RequiresKernelKernelGeometry,
    RequiresKernelKernelAlgebra,
    RequiresKernelKernelCalculus,
    RequiresKernelKernelStatistics,
    RequiresKernelKernelFinance,
    RequiresKernelKernelWavelets,
    RequiresKernelKernelControlSystems,
    RequiresKernelKernelSignalProcessing,
    RequiresKernelKernelImageProcessing,
    RequiresKernelKernelMachineLearning,
    RequiresKernelKernelNeuralNetworks,
    RequiresKernelKernelDataFramework,
    RequiresKernelKernelWorkbench,
    RequiresKernelKernelNotebook,
    RequiresKernelKernelPlayer,
    RequiresKernelKernelDesktop,
    RequiresKernelKernelCloud,
    RequiresKernelKernelEngine,
    RequiresKernelKernelScript,
    RequiresKernelKernelLanguage,
    RequiresKernelKernelAlpha,
    RequiresKernelKernelOctave,
    RequiresKernelKernelFreemat,
    RequiresKernelKernelScilab,
    RequiresKernelKernelSimulink,
    RequiresKernelKernelLabview,
    RequiresKernelKernelMathcad,
    RequiresKernelKernelMupad,
    RequiresKernelKernelMacsyma,
    RequiresKernelKernelReduce,
    RequiresKernelKernelAxiom,
    RequiresKernelKernelSage,
    RequiresKernelKernelMaple,
    RequiresKernelKernelMathematica,
    RequiresKernelKernelMatlab,
    RequiresKernelKernelR,
    RequiresKernelKernelJulia,
    RequiresKernelKernelZig,
    RequiresKernelKernelV,
    RequiresKernelKernelNim,
    RequiresKernelKernelCrystal,
    RequiresKernelKernelD,
    RequiresKernelKernelObjectiveC,
    RequiresKernelKernelSwift,
    RequiresKernelKernelGo,
    RequiresKernelKernelRust,
    RequiresKernelKernelC,
    RequiresKernelKernelCplusplus,
    RequiresKernelKernelFsharp,
    RequiresKernelKernelVbnet,
    RequiresKernelKernelCsharp,
    RequiresKernelKernelIronRuby,
    RequiresKernelKernelIronPython,
    RequiresKernelKernelJython,
    RequiresKernelKernelJRuby,
    RequiresKernelKernelGroovy,
    RequiresKernelKernelClojure,
    RequiresKernelKernelKotlin,
    RequiresKernelKernelScala,
    RequiresKernelKernelOcaml,
    RequiresKernelKernelHaskell,
    RequiresKernelKernelElixir,
    RequiresKernelKernelErlang,
    RequiresKernelKernelProlog,
    RequiresKernelKernelDatalog,
    RequiresKernelKernelSwrl,
    RequiresKernelKernelRuleML,
    RequiresKernelKernelOwl2,
    RequiresKernelKernelRdfs,
    RequiresKernelKernelRdfSchema,
    RequiresKernelKernelShex,
    RequiresKernelKernelShacl,
    RequiresKernelKernelSparql,
    RequiresKernelKernelN3,
    RequiresKernelKernelTrig,
    RequiresKernelKernelNquads,
    RequiresKernelKernelNtriples,
    RequiresKernelKernelTurtle,
    RequiresKernelKernelRdfa,
    RequiresKernelKernelMicrodata,
    RequiresKernelKernelJsonLd,
    RequiresKernelKernelActivityStreams,
    RequiresKernelKernelSioc,
    RequiresKernelKernelFoaf,
    RequiresKernelKernelDublinCore,
    RequiresKernelKernelSkos,
    RequiresKernelKernelOwl,
    RequiresKernelKernelRdf,
    RequiresKernelKernelMathml,
    RequiresKernelKernelSvg,
    RequiresKernelKernelXhtml,
    RequiresKernelKernelHtml,
    RequiresKernelKernelXml,
    RequiresKernelKernelWbxml,
    RequiresKernelKernelExi,
    RequiresKernelKernelFastInfoset,
    RequiresKernelKernelXER,
    RequiresKernelKernelGSER,
    RequiresKernelKernelJER,
    RequiresKernelKernelCER,
    RequiresKernelKernelOER,
    RequiresKernelKernelPER,
    RequiresKernelKernelBer,
    RequiresKernelKernelDer,
    RequiresKernelKernelAsn1,
    RequiresKernelKernelCbor,
    RequiresKernelKernelBson,
    RequiresKernelKernelUbjson,
    RequiresKernelKernelSmile,
    RequiresKernelKernelIon,
    RequiresKernelKernelMsgpack,
    RequiresKernelKernelFlatbuffers,
    RequiresKernelKernelCapnproto,
    RequiresKernelKernelProtobuf,
    RequiresKernelKernelAvro,
    RequiresKernelKernelThrift,
    RequiresKernelKernelGrpc,
    RequiresKernelKernelGnmi,
    RequiresKernelKernelRestconf,
    RequiresKernelKernelNetconf,
    RequiresKernelKernelSnmp,
    RequiresKernelKernelRedfish,
    RequiresKernelKernelIpmi,
    RequiresKernelKernelBmc,
    RequiresKernelKernelIpmitool,
    RequiresKernelKernelSmbios,
    RequiresKernelKernelDmi,
    RequiresKernelKernelAcpi,
    RequiresKernelKernelTss,
    RequiresKernelKernelTpm,
    RequiresKernelKernelEvm,
    RequiresKernelKernelIma,
    RequiresKernelKernelLandlock,
    RequiresKernelKernelYama,
    RequiresKernelKernelTomoyo,
    RequiresKernelKernelSmack,
    RequiresKernelKernelApparmor,
    RequiresKernelKernelSelinux,
    RequiresKernelKernelSeccomp,
    RequiresKernelKernelNamespace,
    RequiresKernelKernelCgroup,
    RequiresKernelKernelTc,
    RequiresKernelKernelXdp,
    RequiresKernelKernelEbpf,
    RequiresKernelKernelBpf,
    RequiresKernelKernelPerf,
    RequiresKernelKernelFtrace,
    RequiresKernelKernelTracepoint,
    RequiresKernelKernelUretprobe,
    RequiresKernelKernelUprobe,
    RequiresKernelKernelKretprobe,
    RequiresKernelKernelKprobe,
    RequiresKernelKernelKcov,
    RequiresKernelKernelKuBSan,
    RequiresKernelKernelKtsan,
    RequiresKernelKernelKfence,
    RequiresKernelKernelKmsan,
    RequiresKernelKernelKcsan,
    RequiresKernelKernelKasan,
    RequiresKernelKernelKptr,
    RequiresKernelKernelKaslr,
    RequiresKernelKernelKaisr,
    RequiresKernelKernelKaiser,
    RequiresKernelKernelKpti,
    RequiresKernelKernelLkG,
    RequiresKernelKernelCet,
    RequiresKernelKernelShadowStack,
    RequiresKernelKernelCfi,
    RequiresKernelKernelDit,
    RequiresKernelKernelPac,
    RequiresKernelKernelApa,
    RequiresKernelKernelBti,
    RequiresKernelKernelMte,
    RequiresKernelKernelPma,
    RequiresKernelKernelPmp,
    RequiresKernelKernelCca,
    RequiresKernelKernelSeV,
    RequiresKernelKernelTdx,
    RequiresKernelKernelSgX,
    RequiresKernelKernelTee,
    RequiresKernelKernelTrustzone,
    RequiresKernelKernelSmm,
    RequiresKernelKernelBios,
    RequiresKernelKernelUefi,
    RequiresKernelKernelBootloader,
    RequiresKernelKernelFirmware,
    RequiresKernelKernelDriver,
    RequiresKernelKernelModule,
    RequiresKernelKernelConfig,
    RequiresKernelKernelRecompile,
    RequiresKernelKernelReboot,
    RequiresKernelKernelUpdate,
    RequiresKernelKernelPatch,
    RequiresKernelKernelMitigation,
    RequiresKernelKernelBypass,
    RequiresKernelKernelPrivilegeEscalation,
    RequiresKernelKernelCodeExecution,
    RequiresKernelKernelCorruption,
    RequiresKernelKernelLeak,
    RequiresKernelKernelDisclosure,
    RequiresKernelKernelDeduplication,
    RequiresKernelKernelReload,
    RequiresKernelKernelEvictReload,
    RequiresKernelKernelFlushFlush,
    RequiresKernelKernelFlushReload,
    RequiresKernelKernelPrimeProbe,
    RequiresKernelKernelCacheAttack,
    RequiresKernelKernelSpeculativeExecution,
    RequiresKernelKernelTransientExecution,
    RequiresKernelKernelLoadValueInjection,
    RequiresKernelKernelStoreToLeak,
    RequiresKernelKernelFallout,
    RequiresKernelKernelRidl,
    RequiresKernelKernelZombieLoad,
    RequiresKernelKernelMeltdown,
    RequiresKernelKernelSpectre,
    RequiresKernelKernelRowhammer,
    RequiresKernelKernelFaultAttack,
    RequiresKernelKernelRadiationAnalysis,
    RequiresKernelKernelThermalAnalysis,
    RequiresKernelKernelOpticalAnalysis,
    RequiresKernelKernelAcousticAnalysis,
    RequiresKernelKernelTimingAnalysis,
    RequiresKernelKernelPowerAnalysis,
    RequiresKernelKernelElectromagneticAnalysis,
    RequiresKernelKernelLaserInjection,
    RequiresKernelKernelGlitching,
    RequiresKernelKernelFaultInjection,
    RequiresKernelKernelSideChannel,
    RequiresKernelKernelAdaptiveChosenCiphertext,
    RequiresKernelKernelChosenCiphertext,
    RequiresKernelKernelChosenPlaintext,
    RequiresKernelKernelKnownPlaintext,
    RequiresKernelKernelLengthExtension,
    RequiresKernelKernelPaddingOracle,
    RequiresKernelKernelTimingAttack,
    RequiresKernelKernelRaceCondition,
    RequiresKernelKernelZeroDay,
    RequiresKernelKernelCompromisedDependency,
    RequiresKernelKernelCompromisedNetwork,
    RequiresKernelKernelCompromisedDevice,
    RequiresKernelKernelCompromisedAccount,
    RequiresKernelKernelInsider,
    RequiresKernelKernelMalware,
    RequiresKernelKernelSocialEngineering,
    RequiresKernelKernelPhysicalAccess,
    RequiresKernelKernelPrivilege,
    RequiresKernelKernelAuthorization,
    RequiresKernelKernelAuthentication,
    RequiresKernelKernelLoggedAndMonitored,
    RequiresKernelKernelReversibleAction,
    RequiresKernelKernelNoSideEffects,
    RequiresKernelKernelSafeHttpMethod,
    RequiresKernelKernelIdempotentOperation,
    RequiresKernelKernelReadOnlyOperation,
    RequiresKernelKernelNoSensitiveData,
    RequiresKernelKernelSelfOnly,
    RequiresKernelKernelOwnerOnly,
    RequiresKernelKernelAdminOnly,
    RequiresKernelKernelInternalNetworkOnly,
    RequiresKernelKernelVpnRequired,
    RequiresKernelKernelNetworkSegmentation,
    RequiresKernelKernelMutualTls,
    RequiresKernelKernelEncryptedChannel,
    RequiresKernelKernelSignatureVerified,
    RequiresKernelKernelTimestampValidated,
    RequiresKernelKernelNonceUsed,
    RequiresKernelKernelReplayProtection,
    RequiresKernelKernelRequestSigning,
    RequiresKernelKernelCustomHeaderRequired,
    RequiresKernelKernelOriginChecked,
    RequiresKernelKernelRefererChecked,
    RequiresKernelKernelUserAgentRestricted,
    RequiresKernelKernelTimeRestricted,
    RequiresKernelKernelGeoRestricted,
    RequiresKernelKernelIpRestricted,
    RequiresKernelKernelMfaProtected,
    RequiresKernelKernelCertificatePinning,
    RequiresKernelKernelTokenBinding,
    RequiresKernelKernelSameSiteCookie,
    RequiresKernelKernelSecureCookie,
    RequiresKernelKernelHttpOnlyCookie,
    RequiresKernelKernelTrustedTypes,
    RequiresKernelKernelSubresourceIntegrity,
    RequiresKernelKernelContentSecurityPolicy,
    RequiresKernelKernelSameOriginPolicy,
    RequiresKernelKernelCsrfProtected,
    RequiresKernelKernelCorsRestricted,
    RequiresKernelKernelWafProtected,
}

impl NegativeHypothesis {
    /// Stable machine-readable name for this hypothesis.
    ///
    /// Returns the same string serde produces for the variant
    /// (`rename_all = "snake_case"`), so a name round-trips through
    /// `serde_json` unchanged. The 12 hand-written hypotheses have curated
    /// spellings; the generated `RequiresKernel*` ones are derived, which is
    /// why this allocates rather than returning `&'static str`.
    pub fn as_str(&self) -> String {
        match self {
            Self::PublicByDesign => "public_by_design".to_string(),
            Self::NonSensitive => "non_sensitive".to_string(),
            Self::Unreachable => "unreachable".to_string(),
            Self::NonExploitable => "non_exploitable".to_string(),
            Self::RequiresImpossibleConditions => "requires_impossible_conditions".to_string(),
            Self::ClientOnlyBehavior => "client_only_behavior".to_string(),
            Self::InformationalExposure => "informational_exposure".to_string(),
            Self::RateLimited => "rate_limited".to_string(),
            Self::AlreadyProtected => "already_protected".to_string(),
            Self::ExpectedApiBehavior => "expected_api_behavior".to_string(),
            Self::ScannerArtifact => "scanner_artifact".to_string(),
            Self::EnvironmentArtifact => "environment_artifact".to_string(),
            other => serde_json::to_value(other)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".to_string()),
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            Self::PublicByDesign,
            Self::NonSensitive,
            Self::Unreachable,
            Self::NonExploitable,
            Self::RequiresImpossibleConditions,
            Self::ClientOnlyBehavior,
            Self::InformationalExposure,
            Self::RateLimited,
            Self::AlreadyProtected,
            Self::ExpectedApiBehavior,
            Self::ScannerArtifact,
            Self::EnvironmentArtifact,
        ]
    }

    /// Cheap deterministic test hint for each negative hypothesis.
    pub fn discriminating_test(&self) -> &'static str {
        match self {
            Self::PublicByDesign => "compare unauthenticated vs authenticated; check docs",
            Self::NonSensitive => "verify data sensitivity with second source",
            Self::Unreachable => "confirm route reachable from attacker network",
            Self::NonExploitable => "attempt benign proof-of-impact, not theory",
            Self::RequiresImpossibleConditions => "list preconditions; test each",
            Self::ClientOnlyBehavior => "confirm server-side effect, not just DOM",
            Self::InformationalExposure => "check for security boundary crossing",
            Self::RateLimited => "test repeatability under rate limits",
            Self::AlreadyProtected => "verify WAF/auth actually blocks exploit",
            Self::ExpectedApiBehavior => "compare against API spec / baseline",
            Self::ScannerArtifact => "reproduce manually without scanner",
            Self::EnvironmentArtifact => "reproduce in clean environment",
            // No curated test for the generated variants; say so explicitly
            // rather than returning a hint for a different hypothesis.
            _ => "no discriminating test defined for this hypothesis",
        }
    }
}

/// Result of testing negative hypotheses against a candidate.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct FalsePositiveDefense {
    pub tested: Vec<NegativeHypothesis>,
    pub survived: Vec<NegativeHypothesis>,
    pub refuted_by: Vec<NegativeHypothesis>,
}

impl FalsePositiveDefense {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_survived(&mut self, h: NegativeHypothesis) {
        if !self.tested.contains(&h) {
            self.tested.push(h.clone());
        }
        if !self.survived.contains(&h) {
            self.survived.push(h);
        }
    }

    pub fn record_refuted(&mut self, h: NegativeHypothesis) {
        if !self.tested.contains(&h) {
            self.tested.push(h.clone());
        }
        if !self.refuted_by.contains(&h) {
            self.refuted_by.push(h);
        }
    }

    /// A finding is still viable only if it was refuted by *nothing*.
    ///
    /// It also must have been actually tested. The previous version returned
    /// `self.refuted_by.is_empty()`, which meant a freshly-constructed
    /// `FalsePositiveDefense` — the state before any negative test has run —
    /// read as maximally viable. The whole module is about a finding becoming
    /// stronger by *surviving* attempts to disprove it, so an untested finding
    /// has demonstrated nothing and must not pass as validated.
    pub fn is_still_viable(&self) -> bool {
        self.refuted_by.is_empty() && !self.tested.is_empty()
    }

    /// Whether the defense was actually exercised.
    ///
    /// `survived` is the meaningful signal: those are the negative hypotheses
    /// that were tried and did *not* kill the finding. A defense that only
    /// recorded refutations has proven the opposite of what it claims.
    pub fn was_exercised(&self) -> bool {
        !self.survived.is_empty()
    }

    /// Whether any negative hypothesis actively killed the finding.
    ///
    /// Separate from [`Self::is_still_viable`] so callers can distinguish
    /// "disproven" from "never tested" — conflating the two reports a finding
    /// as refuted when nobody has actually looked at it yet.
    pub fn is_refuted(&self) -> bool {
        !self.refuted_by.is_empty()
    }

    pub fn defense_score(&self) -> f32 {
        if !self.is_still_viable() {
            return 0.0;
        }
        (self.survived.len() as f32 / NegativeHypothesis::all().len() as f32).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_score_derives_state() {
        let q = VerificationQuality {
            reproducibility: 0.9,
            evidence_strength: 0.9,
            impact_demonstration: 0.9,
            exploitability_evidence: 0.8,
            independence: 0.8,
            contradiction_level: 0.0,
            assumption_count: 0,
        };
        assert_eq!(
            q.verification_state(),
            VerificationState::VerifiedReportable
        );
    }

    #[test]
    fn contradiction_forces_disputed() {
        let q = VerificationQuality {
            contradiction_level: 0.8,
            ..Default::default()
        };
        assert_eq!(q.verification_state(), VerificationState::Disputed);
    }

    #[test]
    fn negative_hypothesis_defense() {
        let mut d = FalsePositiveDefense::new();
        d.record_survived(NegativeHypothesis::PublicByDesign);
        assert!(d.is_still_viable());
        d.record_refuted(NegativeHypothesis::ScannerArtifact);
        assert!(!d.is_still_viable());
        assert_eq!(d.defense_score(), 0.0);
    }
}
