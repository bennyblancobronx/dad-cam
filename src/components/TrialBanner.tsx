// Dad Cam - Trial Banner
// Shows trial countdown at top of app with license key entry CTA

import { useState } from 'react';
import type { LicenseState } from '../types/licensing';
import { LicenseKeyModal } from './modals/LicenseKeyModal';

interface TrialBannerProps {
  licenseState: LicenseState;
  onLicenseChange: (state: LicenseState) => void;
}

export function TrialBanner({ licenseState, onLicenseChange }: TrialBannerProps) {
  const [showKeyModal, setShowKeyModal] = useState(false);

  // Only show for trial licenses
  if (licenseState.licenseType !== 'trial') {
    return null;
  }

  const days = licenseState.trialDaysRemaining ?? 0;
  const isExpired = !licenseState.isActive;

  return (
    <>
      <div className={`trial-banner ${isExpired ? 'trial-banner--expired' : ''}`}>
        <span className="trial-banner-text">
          {isExpired
            ? 'Trial expired -- some features are restricted'
            : `Trial: ${days} day${days !== 1 ? 's' : ''} remaining`}
        </span>
        <button
          className="trial-banner-cta"
          onClick={() => setShowKeyModal(true)}
        >
          Enter License Key
        </button>
      </div>

      {showKeyModal && (
        <LicenseKeyModal
          onClose={() => setShowKeyModal(false)}
          onLicenseChange={onLicenseChange}
        />
      )}
    </>
  );
}
