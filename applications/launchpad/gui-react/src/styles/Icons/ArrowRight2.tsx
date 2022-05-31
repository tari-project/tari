import { SVGProps } from 'react'

const SvgArrowRight2 = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-arrowright2'
    {...props}
  >
    <path
      d='m9.5 7 5 5-5 5'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgArrowRight2
