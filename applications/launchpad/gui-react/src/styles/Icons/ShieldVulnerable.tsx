import * as React from 'react'
import { SVGProps } from 'react'

const SvgShieldVulnerable = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-shieldvulnerable'
    {...props}
  >
    <path
      d='m15.875 19.487.18-.124c.566-.391.849-.587 1.101-.796a7.495 7.495 0 0 0 2.603-4.476c.056-.323.086-.664.144-1.346l.03-.353A17.996 17.996 0 0 0 19.906 9l-.036-.349a5.75 5.75 0 0 0-3.205-4.574 10.642 10.642 0 0 0-9.328 0A5.75 5.75 0 0 0 4.13 8.65l-.036.35a17.993 17.993 0 0 0-.029 3.391l.03.353c.06.682.089 1.023.145 1.346a7.495 7.495 0 0 0 2.603 4.476c.253.21.535.405 1.1.796l.18.124c.769.532 1.153.797 1.538.982a5.41 5.41 0 0 0 4.676 0c.385-.185.77-.45 1.537-.982Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
    <path
      d='m13.768 10.232-3.536 3.536m3.536 0-3.536-3.536'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
  </svg>
)

export default SvgShieldVulnerable
