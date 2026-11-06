import { Card as HeroCard } from "@heroui/react";
import React from "react";

/** HeroUI's card, holding its content in the card's own content part. */
export function Card({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <HeroCard>
      <HeroCard.Content>{children}</HeroCard.Content>
    </HeroCard>
  );
}
