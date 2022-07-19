import { SVGProps } from 'react'

const SvgTariSignet = (props: SVGProps<SVGSVGElement>) => (
  <svg
    xmlns='http://www.w3.org/2000/svg'
    width='34'
    height='35'
    viewBox='0 0 34 35'
    fill='none'
    data-testid='svg-tarisignetgradient'
    {...props}
  >
    <path
      d='M0 9.81162V19.2797L14.0588 34.75L34 19.3341V9.80175L14.1306 0.75L0 9.81162ZM12.3708 27.8624L3.40618 17.9878V12.6226L12.3708 14.8954V27.8624ZM15.7836 29.156V15.766L28.8189 19.0674L15.7836 29.156ZM30.5972 11.9495V16.054L6.16618 9.86099L14.4045 4.58465L30.5972 11.9495Z'
      fill='url(#paint0_linear_3049_49457)'
    />
    <defs>
      <linearGradient
        id='paint0_linear_3049_49457'
        x1='34'
        y1='0.750002'
        x2='-18.6676'
        y2='31.2679'
        gradientUnits='userSpaceOnUse'
      >
        <stop stopColor='#9330FF' />
        <stop offset='1' stopColor='#593A9B' />
      </linearGradient>
    </defs>
  </svg>
)

export default SvgTariSignet
