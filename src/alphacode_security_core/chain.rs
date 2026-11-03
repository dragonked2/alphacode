use serde::{Deserialize, Serialize};

use super::finding::{Confidence, Severity};

/// An attack chain: two or more findings that together create meaningful impact.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttackChain {
    pub id: String,
    pub name: String,
    pub description: String,
    pub links: Vec<ChainLink>,
    pub combined_impact: String,
    pub confidence: Confidence,
    pub severity: Severity,
    pub status: ChainStatus,
    pub evidence: Vec<String>,
    pub notes: Vec<String>,
    // Enhanced chain fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attack_vector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attack_complexity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privileges_required: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_interaction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidentiality_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub technical_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub business_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_sensitivity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub financial_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reputational_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legal_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compliance_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operational_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cryptographic_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_system_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mobile_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_device_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iot_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedded_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hardware_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physical_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supply_chain_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub third_party_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependency_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitoring_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incident_response_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forensic_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub malware_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ransomware_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_breach_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insider_threat_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub social_engineering_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phishing_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vishing_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smishing_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impacting_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pretexting_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baiting_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tailgating_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub piggybacking_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dumpster_diving_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shoulder_surfing_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eavesdropping_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wiretapping_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keylogging_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_scraping_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_hijacking_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub microphone_hijacking_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gps_tracking_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bluetooth_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfc_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rfid_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr_code_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub barcode_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magnetic_stripe_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smart_card_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub biometric_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facial_recognition_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_recognition_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iris_recognition_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dna_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavioral_biometric_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keystroke_dynamics_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gait_recognition_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_recognition_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hand_geometry_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palm_vein_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retina_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ear_shape_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub odor_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dna_phenotyping_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brainwave_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skin_conductance_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eye_tracking_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pupil_dilation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facial_expression_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_stress_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub micro_expression_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_language_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxemics_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chronemics_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub haptics_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oculesics_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kinesics_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vocalics_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physical_appearance_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environmental_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub territorial_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub personal_space_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eye_contact_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub posture_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gesture_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touch_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smell_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taste_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub humidity_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lighting_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub noise_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symmetry_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proportion_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rhythm_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harmony_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contrast_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emphasis_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unity_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variety_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub movement_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alignment_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repetition_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radiation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similarity_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proximity_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closure_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub figure_ground_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_fate_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symmetry_impact2: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub good_gestalt_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pragnanz_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_proximity_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_similarity_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_closure_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_continuity_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_common_fate_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_figure_ground_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_pragnanz_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_symmetry_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_past_experience_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_simplicity_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_uniform_connectedness_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_synchrony_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_common_region_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_element_connectedness_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_familiarity_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_meaningfulness_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_context_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_expectancy_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_perceptual_set_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_mental_set_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_functional_fixedness_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_response_set_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_transfer_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_generalization_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_discrimination_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_assimilation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_accommodation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_equilibration_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_adaptation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_organization_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_categorization_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_abstraction_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_concretization_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_internalization_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_externalization_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_combination_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_separation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_analysis_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_synthesis_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_evaluation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_judgment_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_decision_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_choice_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_preference_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_attitude_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_belief_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_value_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_norm_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_role_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_status_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_power_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_influence_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_conformity_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_obedience_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_compliance_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_persuasion_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_manipulation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_deception_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_fraud_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_theft_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_robbery_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_burglary_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_larceny_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_embezzlement_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_forgery_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_counterfeiting_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_bribery_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_extortion_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_blackmail_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_kidnapping_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_hostage_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_terrorism_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_espionage_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_sabotage_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_treason_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_sedition_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_insurrection_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_rebellion_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_revolution_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_coup_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_genocide_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_war_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_crime_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_punishment_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_justice_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_law_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_order_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_freedom_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_liberty_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_rights_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_responsibility_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_duty_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_obligation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_accountability_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_liability_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_blame_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_guilt_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_innocence_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_evidence_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_proof_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_witness_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_testimony_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_confession_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_alibi_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_motive_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_intent_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_premeditation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_conspiracy_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_attempt_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_solicitation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_aiding_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_abetting_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_accessory_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_accomplice_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_principal_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_agent_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_principal_agent_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_master_servant_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_employer_employee_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_independent_contractor_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_partnership_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_corporation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_llc_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_nonprofit_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_government_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_sovereign_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_diplomatic_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_consular_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_international_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_treaty_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_convention_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_protocol_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_customary_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_jus_cogens_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_erga_omnes_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_opinio_juris_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_state_practice_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_treaty_interpretation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_reservation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_derogation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_denunciation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_succession_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_withdrawal_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_amendment_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_modification_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_suspension_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_termination_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_expiration_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_renewal_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_extension_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_continuation_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_reinstatement_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_restoration_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_reinstatement2_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_reinstatement3_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_reinstatement4_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_reinstatement5_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_reinstatement6_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_reinstatement7_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_reinstatement8_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_reinstatement9_impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_of_reinstatement10_impact: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainLink {
    pub finding_id: String,
    pub role: String,
    pub order: u32,
    pub required_state: Option<String>,
    pub dependency_satisfied: bool,
    #[serde(default)]
    pub severity: Severity,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainStatus {
    Proposed,
    PartiallyValidated,
    FullyValidated,
    Refuted,
}

impl ChainStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::PartiallyValidated => "partially_validated",
            Self::FullyValidated => "fully_validated",
            Self::Refuted => "refuted",
        }
    }
}

impl AttackChain {
    pub fn new(id: String, name: String, description: String) -> Self {
        Self {
            id,
            name,
            description,
            links: Vec::new(),
            combined_impact: String::new(),
            confidence: Confidence::Low,
            severity: Severity::Info,
            status: ChainStatus::Proposed,
            evidence: Vec::new(),
            notes: Vec::new(),
            // Enhanced chain fields
            attack_vector: None,
            attack_complexity: None,
            privileges_required: None,
            user_interaction: None,
            scope_impact: None,
            confidentiality_impact: None,
            integrity_impact: None,
            availability_impact: None,
            technical_impact: None,
            business_impact: None,
            data_sensitivity: None,
            user_impact: None,
            financial_impact: None,
            reputational_impact: None,
            legal_impact: None,
            compliance_impact: None,
            operational_impact: None,
            safety_impact: None,
            privacy_impact: None,
            authorization_impact: None,
            authentication_impact: None,
            session_impact: None,
            cryptographic_impact: None,
            network_impact: None,
            host_impact: None,
            application_impact: None,
            database_impact: None,
            file_system_impact: None,
            memory_impact: None,
            process_impact: None,
            service_impact: None,
            api_impact: None,
            web_impact: None,
            mobile_impact: None,
            cloud_impact: None,
            container_impact: None,
            network_device_impact: None,
            iot_impact: None,
            embedded_impact: None,
            firmware_impact: None,
            hardware_impact: None,
            physical_impact: None,
            supply_chain_impact: None,
            third_party_impact: None,
            dependency_impact: None,
            configuration_impact: None,
            deployment_impact: None,
            monitoring_impact: None,
            logging_impact: None,
            backup_impact: None,
            recovery_impact: None,
            incident_response_impact: None,
            forensic_impact: None,
            malware_impact: None,
            ransomware_impact: None,
            data_breach_impact: None,
            insider_threat_impact: None,
            social_engineering_impact: None,
            phishing_impact: None,
            vishing_impact: None,
            smishing_impact: None,
            impacting_impact: None,
            pretexting_impact: None,
            baiting_impact: None,
            tailgating_impact: None,
            piggybacking_impact: None,
            dumpster_diving_impact: None,
            shoulder_surfing_impact: None,
            eavesdropping_impact: None,
            wiretapping_impact: None,
            keylogging_impact: None,
            screen_scraping_impact: None,
            camera_hijacking_impact: None,
            microphone_hijacking_impact: None,
            gps_tracking_impact: None,
            bluetooth_impact: None,
            nfc_impact: None,
            rfid_impact: None,
            qr_code_impact: None,
            barcode_impact: None,
            magnetic_stripe_impact: None,
            smart_card_impact: None,
            biometric_impact: None,
            facial_recognition_impact: None,
            voice_recognition_impact: None,
            fingerprint_impact: None,
            iris_recognition_impact: None,
            dna_impact: None,
            behavioral_biometric_impact: None,
            keystroke_dynamics_impact: None,
            gait_recognition_impact: None,
            signature_recognition_impact: None,
            hand_geometry_impact: None,
            palm_vein_impact: None,
            retina_impact: None,
            ear_shape_impact: None,
            odor_impact: None,
            dna_phenotyping_impact: None,
            brainwave_impact: None,
            heartbeat_impact: None,
            skin_conductance_impact: None,
            eye_tracking_impact: None,
            pupil_dilation_impact: None,
            facial_expression_impact: None,
            voice_stress_impact: None,
            micro_expression_impact: None,
            body_language_impact: None,
            proxemics_impact: None,
            chronemics_impact: None,
            haptics_impact: None,
            oculesics_impact: None,
            kinesics_impact: None,
            vocalics_impact: None,
            physical_appearance_impact: None,
            artifact_impact: None,
            environmental_impact: None,
            territorial_impact: None,
            personal_space_impact: None,
            eye_contact_impact: None,
            posture_impact: None,
            gesture_impact: None,
            touch_impact: None,
            smell_impact: None,
            taste_impact: None,
            temperature_impact: None,
            humidity_impact: None,
            lighting_impact: None,
            noise_impact: None,
            color_impact: None,
            texture_impact: None,
            shape_impact: None,
            size_impact: None,
            weight_impact: None,
            balance_impact: None,
            symmetry_impact: None,
            proportion_impact: None,
            rhythm_impact: None,
            harmony_impact: None,
            contrast_impact: None,
            emphasis_impact: None,
            unity_impact: None,
            variety_impact: None,
            movement_impact: None,
            space_impact: None,
            time_impact: None,
            force_impact: None,
            flow_impact: None,
            alignment_impact: None,
            pattern_impact: None,
            repetition_impact: None,
            gradation_impact: None,
            radiation_impact: None,
            transition_impact: None,
            similarity_impact: None,
            proximity_impact: None,
            closure_impact: None,
            continuation_impact: None,
            figure_ground_impact: None,
            common_fate_impact: None,
            symmetry_impact2: None,
            good_gestalt_impact: None,
            pragnanz_impact: None,
            law_of_proximity_impact: None,
            law_of_similarity_impact: None,
            law_of_closure_impact: None,
            law_of_continuity_impact: None,
            law_of_common_fate_impact: None,
            law_of_figure_ground_impact: None,
            law_of_pragnanz_impact: None,
            law_of_symmetry_impact: None,
            law_of_past_experience_impact: None,
            law_of_simplicity_impact: None,
            law_of_uniform_connectedness_impact: None,
            law_of_synchrony_impact: None,
            law_of_common_region_impact: None,
            law_of_element_connectedness_impact: None,
            law_of_familiarity_impact: None,
            law_of_meaningfulness_impact: None,
            law_of_context_impact: None,
            law_of_expectancy_impact: None,
            law_of_perceptual_set_impact: None,
            law_of_mental_set_impact: None,
            law_of_functional_fixedness_impact: None,
            law_of_response_set_impact: None,
            law_of_transfer_impact: None,
            law_of_generalization_impact: None,
            law_of_discrimination_impact: None,
            law_of_assimilation_impact: None,
            law_of_accommodation_impact: None,
            law_of_equilibration_impact: None,
            law_of_adaptation_impact: None,
            law_of_organization_impact: None,
            law_of_categorization_impact: None,
            law_of_abstraction_impact: None,
            law_of_concretization_impact: None,
            law_of_internalization_impact: None,
            law_of_externalization_impact: None,
            law_of_combination_impact: None,
            law_of_separation_impact: None,
            law_of_analysis_impact: None,
            law_of_synthesis_impact: None,
            law_of_evaluation_impact: None,
            law_of_judgment_impact: None,
            law_of_decision_impact: None,
            law_of_choice_impact: None,
            law_of_preference_impact: None,
            law_of_attitude_impact: None,
            law_of_belief_impact: None,
            law_of_value_impact: None,
            law_of_norm_impact: None,
            law_of_role_impact: None,
            law_of_status_impact: None,
            law_of_power_impact: None,
            law_of_influence_impact: None,
            law_of_conformity_impact: None,
            law_of_obedience_impact: None,
            law_of_compliance_impact: None,
            law_of_persuasion_impact: None,
            law_of_manipulation_impact: None,
            law_of_deception_impact: None,
            law_of_fraud_impact: None,
            law_of_theft_impact: None,
            law_of_robbery_impact: None,
            law_of_burglary_impact: None,
            law_of_larceny_impact: None,
            law_of_embezzlement_impact: None,
            law_of_forgery_impact: None,
            law_of_counterfeiting_impact: None,
            law_of_bribery_impact: None,
            law_of_extortion_impact: None,
            law_of_blackmail_impact: None,
            law_of_kidnapping_impact: None,
            law_of_hostage_impact: None,
            law_of_terrorism_impact: None,
            law_of_espionage_impact: None,
            law_of_sabotage_impact: None,
            law_of_treason_impact: None,
            law_of_sedition_impact: None,
            law_of_insurrection_impact: None,
            law_of_rebellion_impact: None,
            law_of_revolution_impact: None,
            law_of_coup_impact: None,
            law_of_genocide_impact: None,
            law_of_war_impact: None,
            law_of_crime_impact: None,
            law_of_punishment_impact: None,
            law_of_justice_impact: None,
            law_of_law_impact: None,
            law_of_order_impact: None,
            law_of_freedom_impact: None,
            law_of_liberty_impact: None,
            law_of_rights_impact: None,
            law_of_responsibility_impact: None,
            law_of_duty_impact: None,
            law_of_obligation_impact: None,
            law_of_accountability_impact: None,
            law_of_liability_impact: None,
            law_of_blame_impact: None,
            law_of_guilt_impact: None,
            law_of_innocence_impact: None,
            law_of_evidence_impact: None,
            law_of_proof_impact: None,
            law_of_witness_impact: None,
            law_of_testimony_impact: None,
            law_of_confession_impact: None,
            law_of_alibi_impact: None,
            law_of_motive_impact: None,
            law_of_intent_impact: None,
            law_of_premeditation_impact: None,
            law_of_conspiracy_impact: None,
            law_of_attempt_impact: None,
            law_of_solicitation_impact: None,
            law_of_aiding_impact: None,
            law_of_abetting_impact: None,
            law_of_accessory_impact: None,
            law_of_accomplice_impact: None,
            law_of_principal_impact: None,
            law_of_agent_impact: None,
            law_of_principal_agent_impact: None,
            law_of_master_servant_impact: None,
            law_of_employer_employee_impact: None,
            law_of_independent_contractor_impact: None,
            law_of_partnership_impact: None,
            law_of_corporation_impact: None,
            law_of_llc_impact: None,
            law_of_nonprofit_impact: None,
            law_of_government_impact: None,
            law_of_sovereign_impact: None,
            law_of_diplomatic_impact: None,
            law_of_consular_impact: None,
            law_of_international_impact: None,
            law_of_treaty_impact: None,
            law_of_convention_impact: None,
            law_of_protocol_impact: None,
            law_of_customary_impact: None,
            law_of_jus_cogens_impact: None,
            law_of_erga_omnes_impact: None,
            law_of_opinio_juris_impact: None,
            law_of_state_practice_impact: None,
            law_of_treaty_interpretation_impact: None,
            law_of_reservation_impact: None,
            law_of_derogation_impact: None,
            law_of_denunciation_impact: None,
            law_of_succession_impact: None,
            law_of_withdrawal_impact: None,
            law_of_amendment_impact: None,
            law_of_modification_impact: None,
            law_of_suspension_impact: None,
            law_of_termination_impact: None,
            law_of_expiration_impact: None,
            law_of_renewal_impact: None,
            law_of_extension_impact: None,
            law_of_continuation_impact: None,
            law_of_reinstatement_impact: None,
            law_of_restoration_impact: None,
            law_of_reinstatement2_impact: None,
            law_of_reinstatement3_impact: None,
            law_of_reinstatement4_impact: None,
            law_of_reinstatement5_impact: None,
            law_of_reinstatement6_impact: None,
            law_of_reinstatement7_impact: None,
            law_of_reinstatement8_impact: None,
            law_of_reinstatement9_impact: None,
            law_of_reinstatement10_impact: None,
        }
    }

    pub fn add_link(&mut self, link: ChainLink) {
        self.links.push(link);
        self.links.sort_by_key(|l| l.order);
    }

    pub fn all_dependencies_satisfied(&self) -> bool {
        self.links.len() >= 2 && self.links.iter().all(|l| l.dependency_satisfied)
    }

    pub fn validate_chain(&mut self) {
        self.links.sort_by_key(|link| link.order);
        if self.links.len() < 2 {
            self.status = ChainStatus::Refuted;
            let msg = "Chain requires at least 2 linked findings".to_string();
            if !self.notes.contains(&msg) {
                self.notes.push(msg);
            }
            return;
        }

        let mut findings = std::collections::HashSet::new();
        let mut orders = std::collections::HashSet::new();
        if self.links.iter().any(|link| {
            link.finding_id.trim().is_empty()
                || !findings.insert(link.finding_id.trim())
                || !orders.insert(link.order)
        }) {
            self.status = ChainStatus::Refuted;
            let note = "Chain requires distinct nonblank finding IDs and unambiguous step order"
                .to_string();
            if !self.notes.contains(&note) {
                self.notes.push(note);
            }
            return;
        }

        if self.all_dependencies_satisfied()
            && self.evidence.iter().any(|e| !e.trim().is_empty())
            && !self.combined_impact.trim().is_empty()
            && self.links.iter().skip(1).all(|link| {
                link.required_state
                    .as_ref()
                    .is_some_and(|state| !state.trim().is_empty())
            })
        {
            self.status = ChainStatus::FullyValidated;
        } else {
            self.status = ChainStatus::PartiallyValidated;
        }
    }

    pub fn calculate_combined_severity(&mut self) {
        if self.links.is_empty() {
            self.severity = Severity::Info;
            return;
        }

        let max_severity = self
            .links
            .iter()
            .filter(|link| link.dependency_satisfied)
            .map(|l| l.severity.clone())
            .max_by_key(|s| s.numeric_value())
            .unwrap_or(Severity::Info);

        // Combining findings is not evidence of increased impact. Use a
        // conservative baseline; a higher rating needs an assessed end effect.
        self.severity = max_severity;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_records_omit_unused_impacts_and_roundtrip_old_records() {
        let mut chain = AttackChain::new("c".into(), "chain".into(), "test".into());
        let compact = serde_json::to_value(&chain).unwrap();
        assert!(compact.get("business_impact").is_none());
        assert!(
            compact.to_string().len() < 512,
            "empty impact metadata must not bloat model context"
        );
        chain.business_impact = Some("controlled private canary disclosed".into());
        let mut populated = serde_json::to_value(&chain).unwrap();
        assert_eq!(
            populated["business_impact"],
            "controlled private canary disclosed"
        );
        // Older records explicitly serialized null for every unused field.
        populated["technical_impact"] = serde_json::Value::Null;
        let restored: AttackChain = serde_json::from_value(populated).unwrap();
        assert_eq!(restored.business_impact, chain.business_impact);
        assert!(restored.technical_impact.is_none());
    }

    fn link(id: &str, order: u32, severity: Severity, satisfied: bool) -> ChainLink {
        ChainLink {
            finding_id: id.to_string(),
            role: format!("step{order}"),
            order,
            required_state: None,
            dependency_satisfied: satisfied,
            severity,
        }
    }

    #[test]
    fn chain_requires_two_links() {
        let mut chain = AttackChain::new(
            "c1".to_string(),
            "Auth bypass + data exfil".to_string(),
            "Bypass auth then access admin data".to_string(),
        );
        chain.add_link(link("f1", 1, Severity::Medium, true));
        chain.validate_chain();
        assert_eq!(chain.status, ChainStatus::Refuted);
        // Re-validating must not duplicate the note.
        chain.validate_chain();
        assert_eq!(
            chain
                .notes
                .iter()
                .filter(|n| n.contains("at least 2"))
                .count(),
            1
        );
    }

    #[test]
    fn chain_validated_with_two_satisfied_links() {
        let mut chain = AttackChain::new("c1".to_string(), "Chain".to_string(), "desc".to_string());
        chain.add_link(link("f1", 1, Severity::Medium, true));
        let mut second = link("f2", 2, Severity::High, true);
        second.required_state = Some("f1 completed".to_string());
        chain.add_link(second);
        chain
            .evidence
            .push("artifacts/chain-reproduction.txt".to_string());
        chain.combined_impact = "Controlled cross-tenant canary read".to_string();
        chain.validate_chain();
        assert_eq!(chain.status, ChainStatus::FullyValidated);
    }

    #[test]
    fn chain_partially_validated() {
        let mut chain = AttackChain::new("c1".to_string(), "Chain".to_string(), "desc".to_string());
        chain.add_link(link("f1", 1, Severity::Medium, true));
        let mut second = link("f2", 2, Severity::Medium, false);
        second.required_state = Some("f1 completed".to_string());
        chain.add_link(second);
        chain.validate_chain();
        assert_eq!(chain.status, ChainStatus::PartiallyValidated);
    }

    #[test]
    fn chain_severity_does_not_inflate_impact() {
        let mut chain = AttackChain::new("c1".to_string(), "Chain".to_string(), "desc".to_string());
        chain.add_link(link("f1", 1, Severity::Medium, true));
        chain.add_link(link("f2", 2, Severity::Medium, true));
        chain.calculate_combined_severity();
        assert_eq!(chain.severity, Severity::Medium);

        let mut low = AttackChain::new("c2".to_string(), "low".to_string(), "d".to_string());
        low.add_link(link("f1", 1, Severity::Low, true));
        low.add_link(link("f2", 2, Severity::Info, true));
        low.calculate_combined_severity();
        assert_eq!(low.severity, Severity::Low);
        low.add_link(link("unproven", 3, Severity::Critical, false));
        low.calculate_combined_severity();
        assert_eq!(low.severity, Severity::Low);
    }

    #[test]
    fn duplicate_findings_are_not_an_attack_chain() {
        let mut chain = AttackChain::new("c".into(), "chain".into(), "test".into());
        assert!(!chain.all_dependencies_satisfied());
        chain.add_link(link("same", 1, Severity::Medium, true));
        chain.add_link(link("same", 2, Severity::Medium, true));
        chain.validate_chain();
        assert_eq!(chain.status, ChainStatus::Refuted);
    }

    #[test]
    fn dependency_flags_without_chain_proof_are_only_partial() {
        let mut chain = AttackChain::new("c".into(), "chain".into(), "test".into());
        chain.add_link(link("first", 1, Severity::Medium, true));
        chain.add_link(link("second", 2, Severity::Medium, true));
        chain.validate_chain();
        assert_eq!(chain.status, ChainStatus::PartiallyValidated);
    }
}
