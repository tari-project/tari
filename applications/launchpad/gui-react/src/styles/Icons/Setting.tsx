import { SVGProps } from 'react'

const SvgSetting = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-setting'
    {...props}
  >
    <path
      d='M18.48 18.537H21M4.68 12 3 12.044M4.68 12a2.4 2.4 0 1 0 4.8 0 2.4 2.4 0 0 0-4.8 0Zm5.489.044H21m-8.199-6.493H3m18 0h-2.52M3 18.537h9.801m5.079.063a2.4 2.4 0 1 1-4.8 0 2.4 2.4 0 0 1 4.8 0Zm0-13.2a2.4 2.4 0 1 1-4.8 0 2.4 2.4 0 0 1 4.8 0Z'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
  </svg>
)

export default SvgSetting
