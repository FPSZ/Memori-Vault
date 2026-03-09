import { ReactNode } from "react";
import { motion, type HTMLMotionProps, type Variants } from "framer-motion";

export const MOTION_EASE_OUT: [number, number, number, number] = [0.22, 1, 0.36, 1];

export const fadeSlideUpVariants: Variants = {
  hidden: { opacity: 0, y: 10 },
  show: {
    opacity: 1,
    y: 0,
    transition: { duration: 0.24, ease: MOTION_EASE_OUT }
  },
  exit: {
    opacity: 0,
    y: 8,
    transition: { duration: 0.18, ease: "easeInOut" }
  }
};

export const staggerContainerVariants: Variants = {
  hidden: {},
  show: {
    transition: {
      staggerChildren: 0.035,
      delayChildren: 0.02
    }
  }
};

export const staggerItemVariants: Variants = {
  hidden: { opacity: 0, y: 6 },
  show: {
    opacity: 1,
    y: 0,
    transition: { duration: 0.2, ease: MOTION_EASE_OUT }
  }
};

type AnimatedPressButtonProps = HTMLMotionProps<"button"> & {
  tapScale?: number;
};

export function AnimatedPressButton({
  tapScale = 0.97,
  transition = { type: "spring", stiffness: 360, damping: 26 },
  ...props
}: AnimatedPressButtonProps) {
  return (
    <motion.button
      whileTap={{ scale: tapScale }}
      transition={transition}
      {...props}
    />
  );
}

type AnimatedPanelProps = {
  children: ReactNode;
  className?: string;
};

export function AnimatedPanel({ children, className }: AnimatedPanelProps) {
  return (
    <motion.div
      variants={staggerItemVariants}
      transition={{ duration: 0.2, ease: MOTION_EASE_OUT }}
      className={className}
    >
      {children}
    </motion.div>
  );
}
