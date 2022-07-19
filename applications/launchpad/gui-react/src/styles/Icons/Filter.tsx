import { SVGProps } from 'react'

const SvgFilter = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-filter'
    {...props}
  >
    <path
      d='M17.988 3H5.88C4.254 3 3.308 4.836 4.25 6.16l4.13 5.798a4 4 0 0 1 .742 2.32v4.051a2.67 2.67 0 1 0 5.341 0v-3.99a4 4 0 0 1 .806-2.408l4.316-5.727C20.58 4.886 19.638 3 17.988 3Z'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgFilter
