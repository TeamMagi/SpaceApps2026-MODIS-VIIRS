import { useEffect, useState } from "react";
import { loadValidationGate, validationForRegion } from "../services/validationGate";
import { useAppStore } from "../store/useAppStore";
import type { ValidationGate, ValidationRegionGate } from "../types";

export function useValidationGate(): ValidationRegionGate | null {
  const region = useAppStore((state) => state.region);
  const [gate, setGate] = useState<ValidationGate | null>(null);
  useEffect(() => {
    let active = true;
    void loadValidationGate().then((value) => {
      if (active) setGate(value);
    });
    return () => { active = false; };
  }, []);
  return validationForRegion(gate, region);
}
